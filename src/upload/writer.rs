use std::io::SeekFrom;
use std::path::Path;

use tokio::fs::OpenOptions;
use tokio::io::{AsyncSeekExt, AsyncWriteExt, BufWriter};

const BUF_CAPACITY: usize = 4 * 1024 * 1024; // 4MB

/// 流式分块写入器
pub struct ChunkWriter {
    inner: BufWriter<tokio::fs::File>,
}

impl ChunkWriter {
    /// 打开或创建 .part 文件，seek 到 offset 位置
    pub async fn open(part_path: &Path, offset: u64) -> std::io::Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(part_path)
            .await?;

        let mut file = file;
        if offset > 0 {
            file.seek(SeekFrom::Start(offset)).await?;
        }

        Ok(Self {
            inner: BufWriter::with_capacity(BUF_CAPACITY, file),
        })
    }

    /// 写入数据
    pub async fn write_all(&mut self, data: &[u8]) -> std::io::Result<()> {
        self.inner.write_all(data).await
    }

    /// flush + sync_data (仅同步数据，不同步 metadata)
    pub async fn flush_data(&mut self) -> std::io::Result<()> {
        self.inner.flush().await?;
        self.inner.get_ref().sync_data().await
    }
}
