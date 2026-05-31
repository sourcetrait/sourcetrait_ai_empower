use crate::*;

/// What: read one length-prefixed msgpack frame from a blocking
/// reader. Reads a 4-byte little-endian length, then exactly that
/// many payload bytes, and returns the payload. Surfaces
/// `UnexpectedEof` if the channel closes between or during reads.
///
/// Why: the host<->worker IPC needs a self-delimiting framing so a
/// stream of msgpack messages can be decoded reliably; LE u32 length
/// prefix matches nushell's own plugin protocol and is the simplest
/// shape that works across platforms.
///
/// Where: called by `worker::request_loop::serve` to read each
/// incoming RunRequest from the worker's stdin. The async sibling
/// `read_frame_async` is used on the host side where tokio runs the
/// reader.
pub(crate) fn read_frame<R: Read>(r: &mut R) -> io::Result<Vec<u8>> {
    let mut len_buf = [0u8; 4];
    r.read_exact(&mut len_buf)?;
    let len = u32::from_le_bytes(len_buf) as usize;
    let mut payload = vec![0u8; len];
    r.read_exact(&mut payload)?;
    Ok(payload)
}

/// What: write one length-prefixed msgpack frame to a blocking
/// writer. Emits a 4-byte little-endian length, then the payload,
/// then flushes.
///
/// Why: the matching mate to `read_frame`. The explicit flush is
/// load-bearing because the host blocks reading the response; without
/// flush, buffering could deadlock both sides.
///
/// Where: called by `worker::request_loop::serve` to write the Hello
/// + each RunResponse to the worker's stdout, the only fd the IPC
/// owns (per `eval_source`'s redirect of external command output).
pub(crate) fn write_frame<W: Write>(w: &mut W, payload: &[u8]) -> io::Result<()> {
    let len = payload.len() as u32;
    w.write_all(&len.to_le_bytes())?;
    w.write_all(payload)?;
    w.flush()?;
    Ok(())
}

/// What: async sibling of `read_frame`. Same wire format (LE u32
/// length + payload bytes); awaits each read instead of blocking.
///
/// Why: the host runs inside a tokio runtime so the read of a
/// worker's response cannot block the executor thread; the async
/// variant lets other tasks make progress while a worker is
/// computing.
///
/// Where: called by `server::worker_handle::WorkerHandle::spawn` to
/// read the worker's Hello, and by `send_request` to read each
/// RunResponse from the worker's stdout pipe.
pub(crate) async fn read_frame_async<R>(r: &mut R) -> io::Result<Vec<u8>>
where
    R: tk::AsyncReadExt + Unpin,
{
    let mut len_buf = [0u8; 4];
    r.read_exact(&mut len_buf).await?;
    let len = u32::from_le_bytes(len_buf) as usize;
    let mut payload = vec![0u8; len];
    r.read_exact(&mut payload).await?;
    Ok(payload)
}

/// What: async sibling of `write_frame`. Same wire format (LE u32
/// length + payload + flush).
///
/// Why: the host's `send_request` runs in async context; awaiting the
/// write avoids blocking the tokio executor under load.
///
/// Where: called by `server::worker_handle::WorkerHandle::send_request`
/// to write each RunRequest to the worker's stdin pipe.
pub(crate) async fn write_frame_async<W>(w: &mut W, payload: &[u8]) -> io::Result<()>
where
    W: tk::AsyncWriteExt + Unpin,
{
    let len = payload.len() as u32;
    w.write_all(&len.to_le_bytes()).await?;
    w.write_all(payload).await?;
    w.flush().await?;
    Ok(())
}
