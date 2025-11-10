use std::{error::Error, sync::Arc};
use serenity::{all::{ChannelId, Colour, Context, CreateEmbed, CreateMessage, GuildId, Message, Timestamp}, async_trait};
use songbird::{input::YoutubeDl, Event, EventContext, EventHandler, Songbird, TrackEvent};
use crate::{queue::{Queue, Track}, HttpKey, TrackKey};

pub type CommandResult = std::result::Result<(), Box<dyn Error + Send + Sync>>;

struct TrackErrorNotifier;

#[async_trait]
impl EventHandler for TrackErrorNotifier {
    async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
        if let EventContext::Track(track_list) = ctx {
            for (state, handle) in *track_list {
                println!(
                    "Track {:?} encountered an error: {:?}",
                    handle.uuid(),
                    state.playing
                );
            }
        }

        None
    }
}

//play
pub async fn play(ctx: &Context, msg: &Message) -> CommandResult {
    let (guild_id, channel_id) = {
        let guild = msg.guild(&ctx.cache).unwrap();
        let channel_id = guild
            .voice_states
            .get(&msg.author.id)
            .and_then(|voice_state| voice_state.channel_id);

        (guild.id, channel_id)
    };

    let connect_to = match channel_id {
        Some(channel) => channel,
        None => {
            msg.reply(ctx, "Not in a voice channel").await?;

            return Ok(());
        },
    };

    let manager = songbird::get(ctx)
        .await
        .expect("Songbird Voice client placed in at initialisation.")
        .clone();


    if let Ok(handler_lock) = manager.join(guild_id, connect_to).await {
        // Attach an event handler to see notifications of all track errors.
        let mut handler = handler_lock.lock().await;
        handler.add_global_event(TrackEvent::Error.into(), TrackErrorNotifier);
    }

    if msg.content.split_ascii_whitespace().count() == 1 {
        msg.reply(&ctx.http, "You need to provide a link or search query!").await?;
        return  Ok(());
    }
    let url = msg.content[6..].trim().to_string(); //bad way, should change later 

    let do_search = !url.starts_with("http");

    let http_client = {
        let data = ctx.data.read().await;
        data.get::<HttpKey>()
            .cloned()
            .expect("Guaranteed to exist in the typemap.")
    };

    if let Some(handler_lock) = manager.get(guild_id) {
        let mut handler = handler_lock.lock().await;

        let mut src = if do_search {
            YoutubeDl::new_search(http_client, url)
        } else {
            YoutubeDl::new(http_client, url)
        };

        if let Ok(track) = Track::from_src(&mut src).await {
            let mut data = ctx.data.write().await;
            let queue = data.get_mut::<Queue>().unwrap();
            let should_play = queue.is_empty().await;
            queue.add(track.clone()).await;
            if !should_play {
                let embed = CreateEmbed::new()
                    .title("Added to queue")
                    .colour(Colour::BLUE)
                    .timestamp(Timestamp::now())
                    .field("Title", track.title.clone(), true)
                    .field("Duration", track.get_duration_str().await, true)
                    .thumbnail(track.image);
                
                msg.channel_id.send_message(&ctx.http, CreateMessage::new().embed(embed)).await?;
            } else {
                let song = handler.play_input(src.clone().into());
                data.insert::<TrackKey>(song.clone());
                let embed = CreateEmbed::new()
                    .title("Now playing")
                    .colour(Colour::BLUE)
                    .timestamp(Timestamp::now())
                    .field("Title", track.title.clone(), true)
                    .field("Duration", track.get_duration_str().await, true)
                    .thumbnail(track.image);
                
                msg.channel_id.send_message(&ctx.http, CreateMessage::new().embed(embed)).await?;


                song.add_event(Event::Track(TrackEvent::End), TrackEnd {
                    channel_id: msg.channel_id,
                    guild_id,
                    manager,
                    ctx: ctx.clone(),
                }).unwrap();
            }
        } else {
            msg.channel_id.say(&ctx.http, "Error playing track D:").await?;
        }
    } else {
        msg.channel_id
            .say(&ctx.http, "Not in a voice channel to play in")
            .await?;
    }

    Ok(())
}

struct TrackEnd {
    channel_id: ChannelId,
    guild_id: GuildId,
    manager: Arc<Songbird>,
    ctx: Context,
}

#[async_trait]
impl EventHandler for TrackEnd {
    async fn act(&self, _ctx: &EventContext<'_>) -> Option<Event> {
        let mut data = self.ctx.data.write().await;
        let queue = data.get_mut::<Queue>().unwrap();
        if queue.has_next_track().await {
            let next_track = queue.get_next().await;

            let handler_lock = self.manager.get(self.guild_id).unwrap();
            let mut handler = handler_lock.lock().await;
            let song = handler.play_input(next_track.src.clone().into());

            song.add_event(Event::Track(TrackEvent::End), TrackEnd {
                channel_id: self.channel_id,
                guild_id: self.guild_id,
                manager: self.manager.clone(),
                ctx: self.ctx.clone(),
            }).unwrap();
            
            let embed = CreateEmbed::new()
                .title("Now playing")
                .colour(Colour::BLUE)
                .timestamp(Timestamp::now())
                .field("Title", next_track.title.clone(), true)
                .field("Duration", next_track.get_duration_str().await, true)
                .thumbnail(next_track.image.clone());
            
            self.channel_id.send_message(&self.ctx.http, CreateMessage::new().embed(embed)).await.unwrap();
            data.insert::<TrackKey>(song.clone());
        }
        else {
            queue.queue.clear();
            let handler_lock = self.manager.get(self.guild_id).unwrap();
            let mut handler = handler_lock.lock().await;
            handler.leave().await.unwrap();
        }
        None
    }
}

//skip
pub async fn skip(ctx: &Context, msg: &Message) -> CommandResult {
    let guild_id = msg.guild_id.unwrap();
    let manager = songbird::get(&ctx).await.unwrap();

    if let Some(handler_lock) = manager.get(guild_id) {
        let mut handler = handler_lock.lock().await;
        handler.stop();

        let mut data = ctx.data.write().await;
        let queue = data.get_mut::<Queue>().unwrap();
        if queue.has_next_track().await {
            let next_track = queue.get_next().await;
            let song = handler.play_input(next_track.src.clone().into());
            let embed = CreateEmbed::new()
                .title("Now playing")
                .colour(Colour::BLUE)
                .timestamp(Timestamp::now())
                .field("Title", next_track.title.clone(), true)
                .field("Duration", next_track.get_duration_str().await, true)
                .thumbnail(next_track.image.clone());
            
            msg.channel_id.send_message(&ctx.http, CreateMessage::new().embed(embed)).await.unwrap();
            
            song.add_event(Event::Track(TrackEvent::End), TrackEnd {
                channel_id: msg.channel_id,
                guild_id,
                manager,
                ctx: ctx.clone(),
            }).unwrap();
            data.insert::<TrackKey>(song.clone());
        }
        else {
            queue.queue.clear();
            handler.leave().await?;
        }
    } else {
        msg.channel_id
            .say(&ctx.http, "Not in a voice channel to play in")
            .await?;
    } 
    
    Ok(())
}

//queue
pub async fn queue(ctx: &Context, msg: &Message) -> CommandResult {
    let data = ctx.data.read().await;
    let queue = data.get::<Queue>().unwrap();
    let mut fields = vec![];
    let mut i = 1;
    for track in queue.queue.clone() {
        fields.push(("", format!("{}: {} [{}]", i.to_string(), track.title, track.get_duration_str().await), false));
        i += 1;
    }
    let builder = CreateEmbed::new()
        .title("Queue")
        .color(Colour::RED)
        .timestamp(Timestamp::now())
        .fields(fields);

    let message = CreateMessage::new().embed(builder);
    msg.channel_id.send_message(&ctx.http, message).await?;

    Ok(())
}

//remove
pub async fn remove(ctx: &Context, msg: &Message) -> CommandResult {
    if msg.content.split_ascii_whitespace().count() == 1 {
        msg.reply(&ctx.http, "You need to provide a queue position").await?;
        return  Ok(());
    }

    let index_str = msg.content[8..].trim();
    let index = usize::from_str_radix(index_str, 10).unwrap();
    if index <= 1 {
        msg.reply(&ctx.http, "Cannot remove the current track! Use !skip instead").await?;
        return Ok(());
    }
    let mut data = ctx.data.write().await;
    let queue = data.get_mut::<Queue>().unwrap();
    if index > queue.get_size().await {
        msg.reply(&ctx, format!("Position {} does not exist in queue!", index)).await?;
        return Ok(());
    }

    let track = queue.remove(index - 1).await;
    let embed = CreateEmbed::new()
        .title("Removed from queue")
        .colour(Colour::FABLED_PINK)
        .timestamp(Timestamp::now())
        .field("", track.title, false);
    msg.channel_id.send_message(&ctx.http, CreateMessage::new().embed(embed)).await?;
    Ok(())
}

//pause
pub async fn pause(ctx: &Context, msg: &Message) -> CommandResult {
    let data = ctx.data.write().await;
    let current_song = data.get::<TrackKey>().unwrap();
    current_song.pause().unwrap();
    let queue = data.get::<Queue>().unwrap();
    let track = queue.get_current().await;
    let embed = CreateEmbed::new()
        .title("Paused")
        .colour(Colour::DARK_GREEN)
        .timestamp(Timestamp::now())
        .field("", track.title.clone(), false);
    msg.channel_id.send_message(&ctx.http, CreateMessage::new().embed(embed)).await?;
    Ok(())
}

//unpause
pub async fn unpause(ctx: &Context, msg: &Message) -> CommandResult {
    let data = ctx.data.read().await;
    let current_song = data.get::<TrackKey>().unwrap();
    current_song.play().unwrap();
    let queue = data.get::<Queue>().unwrap();
    let track = queue.get_current().await;
    let embed = CreateEmbed::new()
        .title("Unpaused")
        .colour(Colour::DARK_GREEN)
        .timestamp(Timestamp::now())
        .field("", track.title.clone(), false);
    msg.channel_id.send_message(&ctx.http, CreateMessage::new().embed(embed)).await?;
    Ok(())
}

//help
pub async fn help(ctx: &Context, msg: &Message) -> CommandResult {
    let embed = CreateEmbed::new()
        .title("Help")
        .timestamp(Timestamp::now())
        .colour(Colour::ORANGE)
        .field("!play", "Play/Queue a track from a link or search term", false)
        .field("!skip", "Skip the current track and start the next", false)
        .field("!queue", "View currently queued tracks", false)
        .field("!remove", "Remove a track from the queue", false)
        .field("!pause", "Pause the current track", false)
        .field("!unpause", "Unpause the current track", false);

    msg.channel_id.send_message(&ctx.http, CreateMessage::new().embed(embed)).await?;
    Ok(())
}