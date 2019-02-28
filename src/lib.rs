/// `OrderedStream` provides a simple interface for working with concurrent streams via both methods
/// and iterator types. It will process and cache incoming messages, and allows for individual
/// message iteration as well as as squeeze/mut_chunks methods which will output a `Vec<T>` or
/// `Iterator<Vec<T>>` of the requested length, consisting of sequential message contents regardless
/// of message length. It should be noted that as of this version, all calls to extract data from the
/// stream will consume messages as they're read out. Also of note are the _timeout methods which
/// provide a way to cache incoming messages, and sop function, both of which will cache messages;
/// the former returning on timeout and the latter returning when it processes all pending messages
/// in its queue.
///
/// # Example
/// ```rust
/// use crossbeam_channel::unbounded;
/// use ordered_stream::OrderedStream;
///
/// let msg_one: Vec<u64> = vec![1,2,3,4,5];
/// let msg_two: Vec<u64> = vec![6,7,8,9,10];
///
/// let mut b = true;
///
/// let (tx, rx) = unbounded();
/// let mut os: OrderedStream<u64> = OrderedStream::with_capacity_recvr(128, rx);
///
/// for i in 0..100 {
///     if b {
///         tx.send((i, msg_one.clone())).expect("no");
///     } else {
///         tx.send((i, msg_two.clone())).expect("no");
///     }
///     b = !b;
/// }
/// drop(tx);
///
/// let test_against = vec![1,2,3,4,5,6,7,8,9,10];
/// for msg in os.chunks_mut(10) {
///     assert_eq!(msg, test_against);
/// }
/// ```
pub use crate::ordered_stream::OrderedStream as OrderedStream;

pub mod ordered_stream;
pub mod offset_vec;
