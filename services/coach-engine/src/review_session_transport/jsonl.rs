use std::{io, sync::Arc};

use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    sync::Mutex,
    task::{JoinHandle, JoinSet},
};

use crate::{
    operating_limits::MAX_REVIEW_SESSION_COMMAND_BYTES,
    review_session_processor::{ProcessorCommandAdmission, ProcessorPrincipal},
};

use super::ReviewSessionCommandExecutor;

const JSONL_INGRESS_CAPACITY: usize = 32;

pub struct ReviewSessionJsonlIngress {
    frames: tokio::sync::mpsc::Receiver<io::Result<Vec<u8>>>,
    reader: JoinHandle<()>,
}

impl ReviewSessionJsonlIngress {
    async fn next(&mut self) -> io::Result<Option<Vec<u8>>> {
        match self.frames.recv().await {
            Some(frame) => frame.map(Some),
            None => Ok(None),
        }
    }
}

impl Drop for ReviewSessionJsonlIngress {
    fn drop(&mut self) {
        self.reader.abort();
    }
}

pub fn start_review_session_jsonl_ingress<R>(input: R) -> ReviewSessionJsonlIngress
where
    R: AsyncRead + Unpin + Send + 'static,
{
    let (sender, frames) = tokio::sync::mpsc::channel(JSONL_INGRESS_CAPACITY);
    let reader = tokio::spawn(read_jsonl_frames(input, sender));
    ReviewSessionJsonlIngress { frames, reader }
}

pub async fn run_review_session_jsonl<R, W>(
    executor: Arc<dyn ReviewSessionCommandExecutor>,
    input: R,
    output: W,
) -> io::Result<()>
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let ingress = start_review_session_jsonl_ingress(input);
    run_review_session_jsonl_ingress(executor, ingress, output).await
}

pub async fn run_review_session_jsonl_ingress<W>(
    executor: Arc<dyn ReviewSessionCommandExecutor>,
    mut ingress: ReviewSessionJsonlIngress,
    output: W,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin + Send + 'static,
{
    let output = Arc::new(Mutex::new(output));
    let mut forwarders = JoinSet::new();
    let mut input_closed = false;

    while !input_closed || !forwarders.is_empty() {
        tokio::select! {
            line = ingress.next(), if !input_closed => {
                let Some(line) = line? else {
                    input_closed = true;
                    continue;
                };
                let mut events = executor.clone().submit(
                    ProcessorPrincipal::LocalCoach,
                    ProcessorCommandAdmission::parse(&line),
                );
                let output = output.clone();
                forwarders.spawn(async move {
                    while let Some(event) = events.recv().await {
                        let encoded =
                            crate::review_session_contract::encode_delivery_frame(event);
                        let mut output = output.lock().await;
                        output.write_all(&encoded).await?;
                        output.flush().await?;
                    }
                    io::Result::Ok(())
                });
            }
            result = forwarders.join_next(), if !forwarders.is_empty() => {
                result.expect("a non-empty JoinSet has a task")
                    .map_err(io::Error::other)??;
            }
        }
    }
    Ok(())
}

async fn read_jsonl_frames<R>(mut input: R, sender: tokio::sync::mpsc::Sender<io::Result<Vec<u8>>>)
where
    R: AsyncRead + Unpin,
{
    read_jsonl_frames_with_limit(&mut input, sender, MAX_REVIEW_SESSION_COMMAND_BYTES).await;
}

async fn read_jsonl_frames_with_limit<R>(
    mut input: R,
    sender: tokio::sync::mpsc::Sender<io::Result<Vec<u8>>>,
    max_frame_bytes: usize,
) where
    R: AsyncRead + Unpin,
{
    let mut read_buffer = [0_u8; 8 * 1024];
    let mut frame = Vec::new();
    let mut oversized = false;

    loop {
        let read = match input.read(&mut read_buffer).await {
            Ok(read) => read,
            Err(error) => {
                let _ = sender.send(Err(error)).await;
                return;
            }
        };
        if read == 0 {
            if !frame.is_empty() || oversized {
                let completed = completed_frame(&mut frame, &mut oversized);
                let _ = sender.send(Ok(completed)).await;
            }
            return;
        }

        for byte in &read_buffer[..read] {
            if *byte == b'\n' {
                let completed = completed_frame(&mut frame, &mut oversized);
                if sender.send(Ok(completed)).await.is_err() {
                    return;
                }
            } else if !oversized {
                if frame.len() == max_frame_bytes {
                    frame.clear();
                    oversized = true;
                } else {
                    frame.push(*byte);
                }
            }
        }
    }
}

fn completed_frame(frame: &mut Vec<u8>, oversized: &mut bool) -> Vec<u8> {
    if std::mem::take(oversized) {
        frame.clear();
        Vec::new()
    } else {
        std::mem::take(frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn oversized_frames_are_rejected_without_losing_the_next_command() {
        let (sender, mut frames) = tokio::sync::mpsc::channel(2);
        read_jsonl_frames_with_limit(&b"12345\nok\n"[..], sender, 4).await;

        assert_eq!(frames.recv().await.unwrap().unwrap(), b"");
        assert_eq!(frames.recv().await.unwrap().unwrap(), b"ok");
        assert!(frames.recv().await.is_none());
    }

    #[tokio::test]
    async fn a_frame_at_the_limit_and_an_unterminated_final_frame_are_preserved() {
        let (sender, mut frames) = tokio::sync::mpsc::channel(2);
        read_jsonl_frames_with_limit(&b"1234\nend"[..], sender, 4).await;

        assert_eq!(frames.recv().await.unwrap().unwrap(), b"1234");
        assert_eq!(frames.recv().await.unwrap().unwrap(), b"end");
        assert!(frames.recv().await.is_none());
    }
}
