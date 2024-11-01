use std::env;

use queue::Queue;
use serenity::all::{ActivityData, Context, Message, Ready};
use serenity::async_trait;
use serenity::prelude::*;
use songbird::tracks::TrackHandle;
use songbird::SerenityInit;
use reqwest::Client as HttpClient;

mod commands;
mod queue;

struct HttpKey;

impl TypeMapKey for HttpKey {
    type Value = HttpClient;
}

struct TrackKey;

impl TypeMapKey for TrackKey {
    type Value = TrackHandle;
}

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, ready: Ready) {
        ctx.set_activity(Some(ActivityData::listening("!play")));
        println!("{} is ready!", ready.user.name);
    }

async fn message(&self, ctx: Context, msg: Message) {
        match msg.content.as_str() {
            "!skip" => commands::skip(&ctx, &msg).await.unwrap(),
            "!queue" => commands::queue(&ctx, &msg).await.unwrap(),
            "!help" => commands::help(&ctx, &msg).await.unwrap(),
            s if s.starts_with("!play") => commands::play(&ctx, &msg).await.unwrap(),
            s if s.starts_with("!remove") => commands::remove(&ctx, &msg).await.unwrap(),
            _=> ()
        }
    }
}

#[tokio::main]
async fn main() {
    let token = env::var("music_token").unwrap();

    let intents = GatewayIntents::non_privileged() | GatewayIntents::MESSAGE_CONTENT;

    let mut client = Client::builder(token, intents)
        .event_handler(Handler)
        .register_songbird()
        .type_map_insert::<HttpKey>(HttpClient::new())
        .await.expect("Error building client");



    {
        let mut data = client.data.write().await;
        data.insert::<Queue>(Queue::new().await);
    }

    if let Err(why) = client.start().await {
        println!("Client error: {why:?}");
    }
}
