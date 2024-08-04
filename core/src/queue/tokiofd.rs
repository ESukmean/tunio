use crate::queue::syncfd::SyncFdQueue;
use crate::queue::FdQueueT;
use crate::traits::AsyncQueueT;
use std::io::{self, Read, Write};
use std::os::fd;
use std::os::unix::io::OwnedFd;
use std::pin::Pin;
use std::task::{ready, Context, Poll};
use tokio::io::unix::AsyncFd;
use tokio::io::ReadBuf;
use tokio::io::{AsyncRead, AsyncWrite};

pub struct TokioFdQueue {
    inner: AsyncFd<SyncFdQueue>,
}

impl AsyncQueueT for TokioFdQueue {}

impl FdQueueT for TokioFdQueue {
    const BLOCKING: bool = false;

    fn new(device: OwnedFd) -> Self {
        Self {
            inner: AsyncFd::new(SyncFdQueue::new(device)).unwrap(),
        }
    }
}

impl AsyncRead for TokioFdQueue {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let self_mut = self.get_mut();

        // 이미 guard의 ready!에서 Pending 상태인것을 확인함. 여기서 Err 났다고 바로 리턴하면 Edge Triggering에서 상태를 잃을 수도 있음.

        loop {
            let mut guard = ready!(self_mut.inner.poll_read_ready_mut(cx))?;
            let buffer_ptr = buf.initialize_unfilled();

            match guard.try_io(|inner| inner.get_mut().read(buffer_ptr)) {
                Ok(Ok(size)) => {
                    buf.advance(size);
                    return Poll::Ready(Ok(()));
                }
                Ok(Err(e)) => return Poll::Ready(Err(e)),
                Err(_) => {
                    std::hint::spin_loop();
                }
            }
        }
    }
}

impl AsyncWrite for TokioFdQueue {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let self_mut = self.get_mut();
        loop {
            let mut guard = ready!(self_mut.inner.poll_write_ready_mut(cx))?;

            match guard.try_io(|inner| inner.get_mut().write(buf)) {
                Ok(Ok(n)) => {
                    return Poll::Ready(Ok(n));
                }
                Ok(Err(e)) => return Poll::Ready(Err(e)),
                Err(_) => continue,
            }
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let self_mut = self.get_mut();
        loop {
            let mut guard = ready!(self_mut.inner.poll_write_ready_mut(cx))?;

            match guard.try_io(|inner| inner.get_mut().flush()) {
                Ok(Ok(())) => {
                    return Poll::Ready(Ok(()));
                }
                Ok(Err(e)) => return Poll::Ready(Err(e)),
                Err(_) => continue,
            }
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        unimplemented!("shutdown using await not implemented");
        Poll::Ready(Ok(()))
    }
}
