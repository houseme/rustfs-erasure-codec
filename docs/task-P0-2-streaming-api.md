# P0-2: 流式 API (Read/Write) — 子任务详细文档

> **状态: ✅ 已完成 (2026-06-04)** — 14/14 子任务全部完成，并发 I/O + 并行 codec 已集成
> **当前实现校准 (2026-06-28)** — `StreamOptions` 使用 `StreamIoMode::{Auto, Serial, Parallel}`；旧版 `concurrent_streams: bool` 仅作为历史设计记录，不再对应公开 API。
> **当前实现校准 (2026-10-17)** — 新增 `reconstruct_stream_from`，支持明确缺失标记的独立非 Seek reader/writer；旧 Cursor API 作为共享执行器的适配器保留。以下设计记录不代表所有恢复接口均为外部流。详见[完成度核查](erasure-codec-completion-audit-2026-10-17.md)。
> 文档日期: 2026-05-31
> 预估总工作量: 2-3 周
> 前置依赖: 无

---

## 概述

为大文件 (GB 级) 编解码提供基于 `std::io::Read`/`std::io::Write` 的流式接口，避免将整个文件加载到内存。

## 文件影响范围

| 文件 | 操作 | 说明 |
|------|------|------|
| `src/core/stream.rs` | **新建** | 流式 API 核心实现 |
| `src/core/mod.rs` | 修改 | 导出 stream 模块 |
| `src/lib.rs` | 修改 | 导出 `StreamOptions`, `StreamError` |
| `src/errors.rs` | 修改 | 添加 `StreamError` 类型 |
| `src/tests/mod.rs` | 修改 | 添加流式测试 |

---

## P0-2a: API 设计

### P0-2a-1: StreamOptions 设计

**目标**: 定义流式操作的配置选项

**新建文件**: `src/core/stream.rs`

```rust
/// 流式编解码配置
#[derive(Debug, Clone)]
pub struct StreamOptions {
    /// 每次读/写的块大小 (字节)。默认 4MB。
    ///
    /// 较大的块减少系统调用次数但增加内存使用。
    /// 推荐范围: 256KB ~ 16MB。
    pub block_size: usize,

    /// 流式读写调度模式。默认 Auto。
    ///
    /// Auto 使用保守阈值选择串行或 rayon 并行 I/O；
    /// Serial 强制串行读写；Parallel 强制 rayon 并行读写。
    pub io_mode: StreamIoMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamIoMode {
    Auto,
    Serial,
    Parallel,
}

impl Default for StreamOptions {
    fn default() -> Self {
        Self {
            block_size: 4 * 1024 * 1024, // 4MB
            io_mode: StreamIoMode::Auto,
        }
    }
}

impl StreamOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_block_size(mut self, size: usize) -> Self {
        self.block_size = size.max(1024); // 最小 1KB
        self
    }

    pub fn with_io_mode(mut self, mode: StreamIoMode) -> Self {
        self.io_mode = mode;
        self
    }
}
```

**设计决策**:
- `block_size` 最小 1KB，避免过小块导致过多系统调用
- `io_mode` 默认 `Auto`，避免小块场景不必要的并发开销，同时允许 benchmark 显式固定 `Serial` 或 `Parallel`
- 提供 builder 方法

**预估**: 0.5 天

### P0-2a-2: StreamError 设计

**目标**: 定义流式操作的错误类型

**文件**: `src/core/stream.rs` 或 `src/errors.rs`

```rust
/// 流式操作错误
#[derive(Debug)]
pub struct StreamError {
    /// 出错的分片索引
    pub shard_index: usize,
    /// 错误类型
    pub kind: StreamErrorKind,
}

/// 流式错误类型
#[derive(Debug)]
pub enum StreamErrorKind {
    /// 读取分片数据时出错
    Read(std::io::Error),
    /// 写入分片数据时出错
    Write(std::io::Error),
    /// 编解码错误
    Codec(crate::Error),
}

impl std::fmt::Display for StreamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.kind {
            StreamErrorKind::Read(e) =>
                write!(f, "read error on shard {}: {}", self.shard_index, e),
            StreamErrorKind::Write(e) =>
                write!(f, "write error on shard {}: {}", self.shard_index, e),
            StreamErrorKind::Codec(e) =>
                write!(f, "codec error on shard {}: {}", self.shard_index, e),
        }
    }
}

impl std::error::Error for StreamError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.kind {
            StreamErrorKind::Read(e) => Some(e),
            StreamErrorKind::Write(e) => Some(e),
            StreamErrorKind::Codec(e) => Some(e),
        }
    }
}
```

**预估**: 0.5 天

### P0-2a-3: API Review

**目标**: 确认 API 设计符合 Rust 习惯

**检查项**:
- [ ] `impl Read` vs `&mut dyn Read` vs `Box<dyn Read>` — 选择 `impl Read` (零开销)
- [ ] 是否需要 `async` 版本 — 初始版本仅支持同步
- [x] 是否需要 `Send` bound — 当前 stream I/O helper 在可能并行读写的路径上要求 `Read + Send` / `Write + Send`
- [ ] 错误传播语义 — 某个流出错时是否中止所有流
- [ ] 最后一个块的不等长处理 — 使用实际读取长度

**输出**: 最终 API 签名确认

**预估**: 1 天

---

## P0-2b: 实现 encode_stream

### P0-2b-1: 块读取逻辑

**目标**: 实现从多个 data readers 分块读取数据

**文件**: `src/core/stream.rs`

```rust
/// 从 readers 中读取一个块
///
/// 返回: (实际读取的字节数, 是否所有 reader 都已 EOF)
fn read_block(
    readers: &mut [impl std::io::Read],
    buffers: &mut [Vec<u8>],
    block_size: usize,
) -> Result<(usize, bool), StreamError> {
    let mut max_read = 0;
    let mut all_eof = true;

    for (i, (reader, buf)) in readers.iter_mut().zip(buffers.iter_mut()).enumerate() {
        buf.resize(block_size, 0);
        let mut total_read = 0;

        while total_read < block_size {
            match reader.read(&mut buf[total_read..]) {
                Ok(0) => break, // EOF
                Ok(n) => {
                    total_read += n;
                    all_eof = false;
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(StreamError {
                    shard_index: i,
                    kind: StreamErrorKind::Read(e),
                }),
            }
        }

        buf.truncate(total_read);
        max_read = max_read.max(total_read);
    }

    Ok((max_read, all_eof))
}
```

**关键细节**:
- 处理 `Interrupted` 错误 (EINTR) 自动重试
- 处理短读 (read 返回的字节数 < 请求的字节数)
- 追踪最大读取长度，用于对齐所有 shard

**预估**: 1 天

### P0-2b-2: 编码调用集成

**目标**: 将读取的数据传递给 `encode_sep`

**文件**: `src/core/stream.rs`

```rust
impl ReedSolomon {
    pub fn encode_stream(
        &self,
        data: &mut [impl std::io::Read],
        parity: &mut [impl std::io::Write],
        options: &StreamOptions,
    ) -> Result<(), StreamError> {
        let block_size = options.block_size;
        let mut data_bufs: Vec<Vec<u8>> = (0..self.data_shard_count())
            .map(|_| Vec::with_capacity(block_size))
            .collect();
        let mut parity_bufs: Vec<Vec<u8>> = (0..self.parity_shard_count())
            .map(|_| vec![0u8; block_size])
            .collect();

        loop {
            let (read_len, all_eof) = read_block(data, &mut data_bufs, block_size)?;
            if all_eof { break; }

            // 对齐所有 data buffer 到 read_len
            for buf in data_bufs.iter_mut() {
                buf.resize(read_len, 0);
            }
            for buf in parity_bufs.iter_mut() {
                buf.resize(read_len, 0);
                buf.fill(0);
            }

            // 编码
            let data_refs: Vec<&[u8]> = data_bufs.iter().map(|b| b.as_slice()).collect();
            let mut parity_refs: Vec<&mut [u8]> = parity_bufs.iter_mut()
                .map(|b| b.as_mut_slice()).collect();
            self.encode_sep(&data_refs, &mut parity_refs)
                .map_err(|e| StreamError { shard_index: 0, kind: StreamErrorKind::Codec(e) })?;

            // 写入 parity
            write_parity(parity, &parity_bufs, read_len)?;
        }

        Ok(())
    }
}
```

**预估**: 1 天

### P0-2b-3: parity 写入逻辑

**目标**: 将编码后的 parity blocks 写入 writers

```rust
fn write_parity(
    writers: &mut [impl std::io::Write],
    buffers: &[Vec<u8>],
    len: usize,
) -> Result<(), StreamError> {
    for (i, (writer, buf)) in writers.iter_mut().zip(buffers.iter()).enumerate() {
        let mut written = 0;
        while written < len {
            match writer.write(&buf[written..len]) {
                Ok(0) => return Err(StreamError {
                    shard_index: self.data_shard_count + i,
                    kind: StreamErrorKind::Write(
                        std::io::Error::new(std::io::ErrorKind::WriteZero, "write returned 0")
                    ),
                }),
                Ok(n) => written += n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(StreamError {
                    shard_index: self.data_shard_count + i,
                    kind: StreamErrorKind::Write(e),
                }),
            }
        }
    }
    Ok(())
}
```

**预估**: 1 天

### P0-2b-4: 短读/EOF 处理

**目标**: 正确处理最后一个块的不等长情况

**场景**:
1. 所有 data readers 同时 EOF → 正常结束
2. 部分 data readers 先 EOF → 用零填充短的 shard
3. 某个 reader 返回错误 → 中止并返回 StreamError

**处理**:
```rust
// read_block 中:
// - 短读的 buffer 被 truncate 到实际长度
// - max_read 记录所有 shard 中的最大读取长度
// - 调用方将所有 buffer resize 到 max_read (零填充)
```

**测试**:
- 所有 reader 同时 EOF
- 长度不等的 readers
- reader 中途返回错误

**预估**: 1 天

### P0-2b-5: encode_stream 测试

```rust
#[test]
fn test_encode_stream_basic() {
    let rs = ReedSolomon::new(10, 4).unwrap();
    let data = vec![0u8; 1024 * 1024]; // 1MB
    let mut data_readers: Vec<&[u8]> = vec![data.as_slice(); 10];
    let mut parity_writers: Vec<Vec<u8>> = vec![Vec::new(); 4];

    rs.encode_stream(
        &mut data_readers,
        &mut parity_writers,
        &StreamOptions::default(),
    ).unwrap();

    // 验证 parity 非空
    for pw in &parity_writers {
        assert_eq!(pw.len(), 1024 * 1024);
    }
}

#[test]
fn test_encode_stream_multi_block() {
    let rs = ReedSolomon::new(10, 4).unwrap();
    let data = vec![0xABu8; 10 * 1024 * 1024]; // 10MB
    let opts = StreamOptions::new().with_block_size(1024 * 1024); // 1MB blocks
    // ...
}

#[test]
fn test_encode_stream_empty() {
    let rs = ReedSolomon::new(10, 4).unwrap();
    let data: Vec<&[u8]> = vec![&[]; 10];
    // ...
}

#[test]
fn test_encode_stream_unequal_lengths() {
    // 不同长度的 data readers
    let d0 = vec![0u8; 1000];
    let d1 = vec![0u8; 500];
    // ...
}

#[test]
fn test_encode_stream_reader_error() {
    // 模拟 reader 返回错误
}
```

**预估**: 1 天

---

## P0-2c: 实现 reconstruct_stream

### P0-2c-1: 缺失分片检测

**目标**: 在流式重建中检测哪些分片缺失

```rust
impl ReedSolomon {
    pub fn reconstruct_stream(
        &self,
        shards: &mut [Option<impl std::io::Read + std::io::Write>],
        options: &StreamOptions,
    ) -> Result<(), StreamError> {
        let missing: Vec<usize> = shards.iter().enumerate()
            .filter_map(|(i, s)| if s.is_none() { Some(i) } else { None })
            .collect();

        if missing.len() > self.parity_shard_count() {
            return Err(StreamError {
                shard_index: 0,
                kind: StreamErrorKind::Codec(Error::TooFewShardsPresent),
            });
        }

        // ...
    }
}
```

**预估**: 1 天

### P0-2c-2: 块级重建逻辑

**算法**:
```
循环:
  1. 从非 None 的 shard readers 读取 block_size 字节
  2. 对 None 位置分配零填充 buffer
  3. 调用 self.reconstruct(buffers)
  4. 将重建的 shard blocks 写入对应的 writer (如果非 None)
  5. 继续直到所有 reader EOF
```

**关键挑战**:
- 每个 block 的缺失模式必须一致
- 最后一个 block 的长度可能不一致
- 需要记录每个 shard 的总长度用于校验

**预估**: 2 天

### P0-2c-3: reconstruct_stream 测试

```rust
#[test]
fn test_reconstruct_stream_single_missing() {
    let rs = ReedSolomon::new(10, 4).unwrap();
    // 编码 1MB 数据
    // 将 data[0] 设为 None
    // 重建
    // 验证 data[0] 被正确恢复
}

#[test]
fn test_reconstruct_stream_max_missing() {
    // 缺失 4 个分片 (恰好等于 parity)
}

#[test]
fn test_reconstruct_stream_too_many_missing() {
    // 缺失 5 个 → 错误
}
```

**预估**: 1 天

---

## P0-2d: 实现 verify_stream

### P0-2d-1: 块级验证逻辑

```rust
impl ReedSolomon {
    pub fn verify_stream(
        &self,
        shards: &mut [impl std::io::Read],
        options: &StreamOptions,
    ) -> Result<bool, StreamError> {
        let block_size = options.block_size;
        let mut bufs: Vec<Vec<u8>> = (0..self.total_shard_count())
            .map(|_| Vec::with_capacity(block_size))
            .collect();

        loop {
            let (read_len, all_eof) = read_block_all(shards, &mut bufs, block_size)?;
            if all_eof { break; }

            for buf in bufs.iter_mut() {
                buf.resize(read_len, 0);
            }

            let refs: Vec<&[u8]> = bufs.iter().map(|b| b.as_slice()).collect();
            if !self.verify(&refs).map_err(|e| StreamError {
                shard_index: 0,
                kind: StreamErrorKind::Codec(e),
            })? {
                return Ok(false);
            }
        }

        Ok(true)
    }
}
```

**预估**: 1 天

### P0-2d-2: verify_stream 测试

```rust
#[test]
fn test_verify_stream_valid() { /* ... */ }

#[test]
fn test_verify_stream_corrupted() { /* ... */ }
```

**预估**: 0.5 天

---

## P0-2e: 并发流读写

### P0-2e-1: rayon 并发读取 ✅

**实现**: `read_block_par` 使用 `rayon::par_iter_mut` 并发读取所有 shard。API 签名要求 `R: Read + Send`（`Cursor<Vec<u8>>`、`BufReader<File>` 等均满足）。错误通过 `Mutex<Option<StreamError>>` 收集第一个错误。

```rust
fn read_block_par<R: Read + Send>(
    readers: &mut [R],
    buffers: &mut [Vec<u8>],
    max_len: usize,
) -> Result<(bool, usize), StreamError>
```

### P0-2e-2: rayon 并发写入 ✅

**实现**: `write_block_par` 使用 `rayon::par_iter_mut` 并发写入所有 shard。API 签名要求 `W: Write + Send`。

```rust
fn write_block_par<W: Write + Send>(
    writers: &mut [W],
    buffers: &[Vec<u8>],
    len: usize,
    shard_offset: usize,
) -> Result<(), StreamError>
```

**编解码并发**: `encode_stream` 调用 `encode_sep_par`（rayon 并行编码），`verify_stream` 调用 `verify_par`。`reconstruct_stream` 的读取阶段也已并行化。

### P0-2e-3: 并发流测试 ✅

4 个新测试:
- `test_encode_stream_concurrent`: 4x2 编码 + 验证 roundtrip
- `test_verify_stream_concurrent`: 并发验证（有效 + 损坏数据）
- `test_reconstruct_stream_concurrent`: 并发重建（2 个缺失分片）
- `test_concurrent_stream_large_blocks`: 10x4 配置，1 MiB 数据，256 KiB 块

---

## P0-2f: 文档

### P0-2f-1: README 示例

```markdown
## 流式编码

对于大文件，可以使用流式 API 避免将整个文件加载到内存:

```rust
use rustfs_erasure_codec::ReedSolomon;
use rustfs_erasure_codec::stream::{StreamIoMode, StreamOptions};
use std::fs::File;
use std::io::BufReader;

let rs = ReedSolomon::new(10, 4).unwrap();

// 打开 10 个 data 文件
let mut data_readers: Vec<BufReader<File>> = (0..10)
    .map(|i| BufReader::new(File::open(format!("data_{}.bin", i)).unwrap()))
    .collect();

// 打开 4 个 parity 文件
let mut parity_writers: Vec<File> = (0..4)
    .map(|i| File::create(format!("parity_{}.bin", i)).unwrap())
    .collect();

let opts = StreamOptions::new()
    .with_block_size(4 * 1024 * 1024) // 4MB blocks
    .with_io_mode(StreamIoMode::Auto);

rs.encode_stream(
    &mut data_readers,
    &mut parity_writers,
    &opts,
).unwrap();
```
```

**预估**: 0.5 天

### P0-2f-2: doc comments

为 `StreamOptions`, `StreamError`, `StreamErrorKind`, `encode_stream`, `reconstruct_stream`, `verify_stream` 添加完整的文档注释。

**预估**: 0.5 天

---

## 依赖关系

```
P0-2a-1 + P0-2a-2 → P0-2a-3 (API review)
P0-2a-3 → P0-2b-1 → P0-2b-2 → P0-2b-3 → P0-2b-4 → P0-2b-5
P0-2b-2 → P0-2c-1 → P0-2c-2 → P0-2c-3
P0-2b-2 → P0-2d-1 → P0-2d-2
P0-2b-2 → P0-2e-1 + P0-2e-2 → P0-2e-3
P0-2b-5 + P0-2c-3 + P0-2d-2 + P0-2e-3 → P0-2f-1 + P0-2f-2
```
