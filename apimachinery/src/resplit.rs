use futures::{stream, Async, Poll, Sink, Stream};
use std::mem;

#[derive(Debug)]
#[must_use = "streams do nothing unless polled"]
pub struct ReSplit<S, I, P>
where
    S: Stream,
    S::Item: IntoIterator<Item = I>,
    P: FnMut(&I) -> bool,
{
    buf: Vec<I>,
    err: Option<S::Error>,
    stream: stream::Fuse<S>,
    predicate: P,
}

pub fn new<S, I, P>(s: S, predicate: P) -> ReSplit<S, I, P>
where
    S: Stream,
    S::Item: IntoIterator<Item = I>,
    P: FnMut(&I) -> bool,
{
    ReSplit {
        buf: Vec::new(),
        err: None,
        stream: s.fuse(),
        predicate: predicate,
    }
}

impl<S, I, P> Sink for ReSplit<S, I, P>
where
    S: Sink + Stream,
    S::Item: IntoIterator<Item = I>,
    P: FnMut(&I) -> bool,
{
    type SinkItem = S::SinkItem;
    type SinkError = S::SinkError;

    fn start_send(&mut self, item: S::SinkItem) -> ::futures::StartSend<S::SinkItem, S::SinkError> {
        self.stream.start_send(item)
    }

    fn poll_complete(&mut self) -> Poll<(), S::SinkError> {
        self.stream.poll_complete()
    }

    fn close(&mut self) -> Poll<(), S::SinkError> {
        self.stream.close()
    }
}

impl<S, I, P> Stream for ReSplit<S, I, P>
where
    S: Stream,
    S::Item: IntoIterator<Item = I>,
    P: FnMut(&I) -> bool,
{
    type Item = Vec<I>;
    type Error = S::Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        loop {
            if let Some(i) = self.buf.iter().position(&mut self.predicate) {
                // found separator, so return prefix (including separator)
                let tail = self.buf.split_off(i + 1);
                let head = mem::replace(&mut self.buf, tail);
                return Ok(Some(head).into());
            }

            if let Some(err) = self.err.take() {
                if self.buf.len() > 0 {
                    // flush any buffer first
                    self.err = Some(err);
                    let buf = mem::replace(&mut self.buf, Vec::new());
                    return Ok(Some(buf).into());
                }
                return Err(err);
            }

            match self.stream.poll() {
                Ok(Async::NotReady) => return Ok(Async::NotReady),

                Ok(Async::Ready(Some(item))) => {
                    // New data has arrived, so append to buffer
                    self.buf.extend(item);
                }

                Ok(Async::Ready(None)) => {
                    // Underlying stream ran out of values, so return what we have
                    return if self.buf.len() > 0 {
                        let buf = mem::replace(&mut self.buf, Vec::new());
                        Ok(Some(buf).into())
                    } else {
                        Ok(Async::Ready(None))
                    };
                }

                Err(e) => {
                    self.err = Some(e);
                }
            }
        }
    }
}
