// Copyright 2021 Nathan (Blaise) Bruer.  All rights reserved.

use std::marker::Send;
use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, WriteHalf};

use async_fixed_buffer::AsyncFixedBuf;
use common::DigestInfo;
use error::{error_if, Error, ResultExt};
use traits::{ResultFuture, StoreTrait};

pub struct VerifyStore {
    inner_store: Arc<dyn StoreTrait>,
    verify_size: bool,
}

impl VerifyStore {
    pub fn new(config: &config::backends::VerifyStore, inner_store: Arc<dyn StoreTrait>) -> Self {
        VerifyStore {
            inner_store: inner_store,
            verify_size: config.verify_size,
        }
    }

    fn pin_inner<'a>(&'a self) -> std::pin::Pin<&'a dyn StoreTrait> {
        Pin::new(self.inner_store.as_ref())
    }
}

async fn inner_check_update<'a>(
    mut tx: WriteHalf<AsyncFixedBuf<Box<[u8]>>>,
    mut reader: Box<dyn AsyncRead + Send + Sync + Unpin + 'a>,
    expected_size: i64,
    verify_size: bool,
) -> Result<(), Error> {
    let mut buffer = vec![0u8; 1024 * 4];
    let mut sum_size: i64 = 0;
    loop {
        let sz = reader
            .read(&mut buffer[..])
            .await
            .err_tip(|| "Stream read terminated early")?;
        sum_size += sz as i64;
        error_if!(
            verify_size && sum_size != expected_size,
            "Expected size {} but got size {} on insert",
            expected_size,
            sum_size
        );
        tx.write_all(&buffer[0..sz])
            .await
            .err_tip(|| "Failed to write to underlying store")?;
        if sz != 0 {
            continue;
        }
        // Note: EOF is not sent from write_all() only sent in write().
        tx.write(&vec![]).await.err_tip(|| "Failed to write EOF byte")?;
        tx.shutdown()
            .await
            .err_tip(|| "Failed to shutdown underlying verify_size stream")?;
        return Ok(());
    }
}

#[async_trait]
impl StoreTrait for VerifyStore {
    fn has<'a>(self: std::pin::Pin<&'a Self>, digest: DigestInfo) -> ResultFuture<'a, bool> {
        Box::pin(async move { self.pin_inner().has(digest).await })
    }

    fn update<'a>(
        self: std::pin::Pin<&'a Self>,
        digest: DigestInfo,
        reader: Box<dyn AsyncRead + Send + Sync + Unpin + 'a>,
    ) -> ResultFuture<'a, ()> {
        let expected_size = digest.size_bytes;
        Box::pin(async move {
            let mut raw_fixed_buffer = AsyncFixedBuf::new(vec![0u8; 1024 * 4].into_boxed_slice());
            let mut stream_closer = raw_fixed_buffer.get_closer();
            let (rx, tx) = tokio::io::split(raw_fixed_buffer);

            let inner_store_clone = self.inner_store.clone();
            let spawn_future =
                tokio::spawn(async move { Pin::new(inner_store_clone.as_ref()).update(digest, Box::new(rx)).await });
            let result = inner_check_update(tx, reader, expected_size, self.verify_size).await;
            stream_closer();
            result.merge(
                spawn_future
                    .await
                    .err_tip(|| "Failed to join verify size spawn")
                    .and_then(|v| v),
            )
        })
    }

    fn get_part<'a>(
        self: std::pin::Pin<&'a Self>,
        digest: DigestInfo,
        writer: &'a mut (dyn AsyncWrite + Send + Unpin + Sync),
        offset: usize,
        length: Option<usize>,
    ) -> ResultFuture<'a, ()> {
        Box::pin(async move { self.pin_inner().get_part(digest, writer, offset, length).await })
    }
}