//! 安全分享链接：对文件/目录生成带有效期、可选提取码、仅下载标志的公开链接。

pub mod manager;
pub mod record;

pub use manager::ShareManager;
