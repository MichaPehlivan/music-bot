use std::time::Duration;

use songbird::{input::{AudioStreamError, Compose, YoutubeDl}, typemap::TypeMapKey};

#[derive(Clone)]
pub struct Track {
    pub src: YoutubeDl,
    pub title: String,
    duration: Duration,
    pub image: String,
}

impl Track {
    pub async fn from_src(src: &mut YoutubeDl) -> Result<Track, AudioStreamError> {
        let metadata = src.aux_metadata().await?;
        let title = metadata.title.unwrap();
        let duration = metadata.duration.unwrap();
        let image = metadata.thumbnail.unwrap();
        Ok(Track {
            src: src.clone(),
            title,
            duration,
            image,
        })
    }

    pub async fn get_duration_str(&self) -> String {
        let duration = self.duration;
        let in_secs = duration.as_secs();
        let secs_remainder = in_secs % 60;
        let minutes = (in_secs - secs_remainder) / 60;
        format!("{}:{}{}", minutes, if secs_remainder < 10 {"0"} else {""},secs_remainder)
    }
}

pub struct Queue {
    pub queue: Vec<Track>,
}

impl Queue {
    pub async fn new() -> Self {
        let queue = Vec::new();
        Queue {
            queue,
        }
    }

    pub async fn add(&mut self, track: Track) {
        self.queue.push(track);
    }

    pub async fn get_current(&self) -> &Track {
        self.queue.get(0).unwrap()
    }

    pub async fn get_next(&mut self) -> &Track {
        self.queue.remove(0);
        self.queue.get(0).unwrap()
    }

    pub async fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    pub async fn has_next_track(&self) -> bool {
        self.queue.len() > 1
    }

    pub async fn get_size(&self) -> usize {
        self.queue.len()
    }

    pub async fn remove(&mut self, index: usize) -> Track {
        self.queue.remove(index)
    }    
}

impl TypeMapKey for Queue {
    type Value = Queue;
}