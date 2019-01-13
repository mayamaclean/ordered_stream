use crossbeam_channel::{RecvError, RecvTimeoutError, Receiver, TryRecvError};
use intmap::IntMap;
use log::{info, trace};
use std::result::Result;
use std::time::{Duration, Instant};
use crate::offset_vec::OffsetVec;

pub struct OrderedStream<T: Clone> {
    lookup:   IntMap<OffsetVec<T>>,
    curr:     u64,
    incoming: Receiver<(u64, Vec<T>)>,
}

impl<T: Clone> OrderedStream<T> {
    pub fn with_capacity_recvr(cap: usize, rx: Receiver<(u64, Vec<T>)>)
        -> OrderedStream<T>
    {
        OrderedStream {
            lookup: IntMap::with_capacity(cap),
            curr: 0,
            incoming: rx,
        }
    }

    pub fn with_recvr(rx: Receiver<(u64, Vec<T>)>)
        -> OrderedStream<T>
    {
        OrderedStream::with_capacity_recvr(16, rx)
    }

    pub fn set_recv(&mut self, rx: Receiver<(u64, Vec<T>)>)
    {
        self.incoming = rx;
    }

    pub fn add_item_sep(&mut self, seq: u64, item: Vec<T>)
        -> bool
    {
        self.lookup.insert(seq, OffsetVec::from_pieces(item, 0))
    }

    pub fn add_item(&mut self, msg: (u64, Vec<T>))
        -> bool
    {
        self.add_item_sep(msg.0, msg.1)
    }

    pub fn add_item_sep_offset(&mut self, seq: u64, item: Vec<T>, offset: usize)
        -> bool
    {
        self.lookup.insert(seq, OffsetVec::from_pieces(item, offset))
    }

    pub fn add_item_offset(&mut self, msg: (u64, Vec<T>), offset: usize)
        -> bool
    {
        self.lookup.insert(msg.0, OffsetVec::from_pieces(msg.1, offset))
    }

    pub fn chk_lookup(&mut self, seq: u64)
        -> bool
    {
        self.lookup.contains_key(seq)
    }

    pub fn rm_item(&mut self, seq: u64)
        -> Option<OffsetVec<T>>
    {
        self.lookup.remove(seq)
    }

    pub fn get_item(&mut self, seq: u64)
        -> Option<&mut OffsetVec<T>>
    {
        self.lookup.get_mut(seq)
    }

    pub fn read_to_pos(&mut self, p: u64)
        -> Result<bool, RecvError>//
    {
        if p < self.pos()
        { return Ok(false); }

        if self.chk_lookup(p)
        { return Ok(false); }

        trace!("read_to_pos, {}", p);

        loop {
            match self
                 .incoming
                 .recv()
            {
                Ok(msg) => {
                    let n = msg.0.clone();
                    let b = self.add_item(msg);
                    if n == p { return Ok(b); }
                },
                Err(e) => return Err(e),
            }
        }
    }

    pub fn read_to_seq_pos(&mut self)
        -> Result<bool, RecvError>
    {
        let i = self.pos();
        self.read_to_pos(i)
    }

    pub fn read_to_pos_timeout(&mut self, p: u64, d: Duration)
        -> Result<bool, RecvTimeoutError>
    {
        if p < self.pos() { return Ok(false); }
        if self.chk_lookup(p) { return Ok(false); }

        trace!("read_to_pos_timeout, {}, {:#?}", p, d);

        loop {
            match self
                 .incoming
                 .recv_timeout(d)
            {
                Ok(msg) => {
                    let n = msg.0.clone();
                    let b = self.add_item(msg);
                    if n == p { return Ok(b); }
                },
                Err(e) => return Err(e),
            }
        }
    }

    pub fn read_to_seq_pos_timeout(&mut self, d: Duration)
        -> Result<bool, RecvTimeoutError>
    {
        let i = self.pos();
        self.read_to_pos_timeout(i, d)
    }

    pub fn read_msgs(&mut self, cnt: usize)
        -> (usize, Option<RecvError>)
    {
        trace!("read_msgs {}", cnt);
        let mut a = 0;
        let mut o = None;

        for _ in 0..cnt {
            match self
                 .incoming
                 .recv()
            {
                Ok(msg) => {
                    a += msg.1.len();
                    self.add_item(msg);
                    ()
                },
                Err(e)  => {
                    o = Some(e);
                    break;
                }
            }
        }
        (a, o)
    }

    pub fn read_len(&mut self, len: usize)
        -> (usize, Option<RecvError>)
    {
        trace!("read_len {}", len);
        let mut total = 0;

        while total < len {
            let r =
                self.read_msgs(1);
            match r.1 {
                Some(r) => return (total, Some(r)),
                None => {
                    total += r.0
                }
            }
        }
        (total, None)
    }

    pub fn increment(&mut self)
    {
        self.curr += 1;
    }

    pub fn pos(&self)
        -> u64
    {
        self.curr.clone()
    }

    pub fn squeeze(&mut self, len: usize)
        -> Result<Vec<T>, Option<RecvError>>
    {
        trace!("squeeze {}", len);
        let timer = Instant::now();

        let mut buf: Vec<T>
            = Vec::with_capacity(len);

        let mut remainder
            = len;

        let mut tempcur
            = self.pos();

        let ptr: *mut OrderedStream<T> = self as *mut _;

        match self
             .get_item(tempcur)
        {
            Some(data) => {
                if data.len() == remainder {
                    unsafe { (*ptr).increment() };
                    if data.offset() == 0 {
                        info!("{:#?}", timer.elapsed());
                        unsafe { return Ok((*ptr).rm_item(tempcur).unwrap().into_inner()) };
                    }
                    buf.extend_from_slice(&data[..]);
                    unsafe { (*ptr).rm_item(tempcur) };
                    info!("{:#?}", timer.elapsed());
                    return Ok(buf);
                }
                else if data.len() > remainder {
                    buf.extend_from_slice(&data[..(remainder + data.offset())]);

                    info!("{:#?}", timer.elapsed());

                    data.inc_offset(remainder)
                        .expect("error setting offset");

                    return Ok(buf);
                }
                remainder -= data.len();
                buf.extend_from_slice(&data[..]);
                unsafe { (*ptr).rm_item(tempcur) };

                unsafe { (*ptr).increment() };
                tempcur += 1;
            },
            None => {
                let e = unsafe { (*ptr).read_to_seq_pos() };

                match e {
                    Ok(_) => (),
                    Err(err) => {
                        return Err(Some(err));
                    },
                }
            },
        }

        loop {
            match self
                 .get_item(tempcur)
            {
                Some(data) => {
                    if data.len() == remainder {
                        unsafe { (*ptr).increment() };
                        buf.extend_from_slice(&data[..]);
                        unsafe { (*ptr).rm_item(tempcur) };
                        info!("{:#?}", timer.elapsed());
                        return Ok(buf);
                    }
                    else if data.len() > remainder {
                        buf.extend_from_slice(&data[..(data.offset() + remainder)]);
                        data.inc_offset(remainder)
                            .expect("error setting offset");

                        info!("{:#?}", timer.elapsed());

                        return Ok(buf);
                    }
                    remainder -= data.len();
                    buf.extend_from_slice(&data[..]);
                    unsafe { (*ptr).rm_item(tempcur) };

                    unsafe { (*ptr).increment() };
                    tempcur += 1;
                },
                None => {
                    let e = unsafe { (*ptr).read_to_seq_pos() };

                    match e {
                        Ok(_) => (),
                        Err(err) => {
                            if buf.len() == 0 {
                                info!("{:#?}", timer.elapsed());
                                return Err(Some(err));
                            }
                            info!("{:#?}", timer.elapsed());
                            return Ok(buf);
                        }
                    }
                },
            }
        }
    }

    pub fn squeeze_timeout(&mut self, len: usize, d: Duration)
        -> Result<Vec<T>, Option<RecvTimeoutError>>
    {
        trace!("squeeze_timeout {}, {:#?}", len, d);
        let timer = Instant::now();

        let mut buf: Vec<T>
            = Vec::with_capacity(len);

        let mut remainder
            = len;

        let mut tempcur
            = self.pos();

        let ptr: *mut OrderedStream<T> = self as *mut _;

        match self
             .get_item(tempcur)
        {
            Some(data) => {
                if data.len() == remainder {
                    unsafe { (*ptr).increment() };
                    if data.offset() == 0 {
                        info!("{:#?}", timer.elapsed());
                        unsafe { return Ok((*ptr).rm_item(tempcur).unwrap().into_inner()); };
                    }
                    buf.extend_from_slice(&data[..]);
                    unsafe { (*ptr).rm_item(tempcur) };
                    info!("{:#?}", timer.elapsed());
                    return Ok(buf);
                }
                else if data.len() > remainder {
                    buf.extend_from_slice(&data[..(remainder + data.offset())]);

                    info!("{:#?}", timer.elapsed());

                    data.inc_offset(remainder)
                        .expect("error setting offset");

                    return Ok(buf);
                }
                remainder -= data.len();
                buf.extend_from_slice(&data[..]);
                unsafe { (*ptr).rm_item(tempcur) };

                unsafe { (*ptr).increment() };
                tempcur += 1;
            },
            None => {
                let e = unsafe { (*ptr).read_to_seq_pos_timeout(d) };

                match e {
                    Ok(_) => (),
                    Err(err) => {
                        info!("{:#?}", timer.elapsed());
                        return Err(Some(err));
                    },
                }
            },
        }

        loop {
            match self
                 .get_item(tempcur)
            {
                Some(data) => {
                    if data.len() == remainder {
                        unsafe { (*ptr).increment() };
                        buf.extend_from_slice(&data[..]);
                        unsafe { (*ptr).rm_item(tempcur) };
                        info!("{:#?}", timer.elapsed());
                        return Ok(buf);
                    }
                    else if data.len() > remainder {
                        buf.extend_from_slice(&data[..(data.offset() + remainder)]);
                        data.inc_offset(remainder)
                            .expect("error setting offset");

                        info!("{:#?}", timer.elapsed());

                        return Ok(buf);
                    }
                    remainder -= data.len();
                    buf.extend_from_slice(&data[..]);
                    unsafe { (*ptr).rm_item(tempcur) };

                    unsafe { (*ptr).increment() };
                    tempcur += 1;
                },
                None => {
                    let e = unsafe { (*ptr).read_to_seq_pos_timeout(d) };

                    match e {
                        Ok(_) => (),
                        Err(err) => {
                            if buf.len() == 0 {
                                info!("{:#?}", timer.elapsed());
                                return Err(Some(err));
                            }
                            info!("{:#?}", timer.elapsed());
                            return Ok(buf);
                        }
                    }
                },
            }
        }
    }

    pub fn current(&mut self)
        -> Result<Vec<T>, Option<RecvError>>
    {
        trace!("current");
        let timer = Instant::now();
        let i = self.pos();

        match self
             .rm_item(i)
        {
            Some(item) => {
                self.increment();
                info!("{:#?}", timer.elapsed());
                return Ok(item.into_inner());
            },
            None => {
                match self.read_to_seq_pos() {
                    Ok(b) => {
                        if b {
                            self.increment();
                            info!("{:#?}", timer.elapsed());
                            return Ok(self.rm_item(i).unwrap().into_inner());
                        }
                        info!("{:#?}", timer.elapsed());
                        return Err(None);
                    },
                    Err(e) => {
                        info!("{:#?}", timer.elapsed());
                        return Err(Some(e));
                    },
                }
            }
        }
    }

    pub fn current_timeout(&mut self, d: Duration)
        -> Result<Vec<T>, Option<RecvTimeoutError>>
    {
        trace!("current_timeout {:#?}", d);
        let timer = Instant::now();
        let i = self.pos();

        match self
             .rm_item(i)
        {
            Some(item) => {
                self.increment();
                info!("{:#?}", timer.elapsed());
                return Ok(item.into_inner());
            },
            None => {
                match self
                     .read_to_seq_pos_timeout(d)
                {
                    Ok(b) => {
                        if b == true {
                            self.increment();
                            info!("{:#?}", timer.elapsed());

                            return Ok(self.rm_item(i).unwrap().into_inner());
                        }
                        info!("{:#?}", timer.elapsed());
                        return Err(None);
                    },
                    Err(e) => {
                        info!("{:#?}", timer.elapsed());
                        return Err(Some(e));
                    },
                }
            }
        }
    }

    pub fn sop(&mut self)
        -> Result<usize, TryRecvError>
    {
        trace!("sop");
        let mut cnt = 0;
        loop {
            match self
                 .incoming
                 .try_recv()
            {
                Ok(item) => {
                    self.add_item(item);
                    cnt += 1;
                },
                Err(e) => {
                    if cnt > 0 {
                        return Ok(cnt);
                    }
                    return Err(e);
                },
            }
        }
    }

    pub fn read_until_timeout(&mut self, d: Duration)
        -> Result<usize, RecvTimeoutError>
    {
        trace!("read_until_timeout {:#?}", d);
        let mut cnt = 0;
        loop {
            match self
                 .incoming
                 .recv_timeout(d)
            {
                Ok(item) => {
                    self.add_item(item);
                    cnt += 1;
                },
                Err(e) => {
                    if cnt > 0 {
                        return Ok(cnt);
                    }
                    return Err(e);
                },
            }
        }
    }

    pub fn read_for_duration(&mut self, d: Duration)
        -> Result<usize, TryRecvError>
    {
        trace!("read_for_duration {:#?}", d);
        let timer = Instant::now();

        let mut cnt = 0;
        loop {
            match self
                 .incoming
                 .try_recv()
            {
                Ok(item) => {
                    self.add_item(item);
                    cnt += 1;
                    if timer.elapsed() >= d {
                        return Ok(cnt);
                    }
                },
                Err(e) => {
                    if cnt > 0 {
                        if timer.elapsed() >= d {
                            return Ok(cnt);
                        }
                    } else {
                        if timer.elapsed() >= d {
                            return Err(e);
                        }
                    }
                },
            }
        }
    }

    pub fn mut_chunks(&mut self, sz: usize)
        -> OrderedChunks<T>
    {
        OrderedChunks {
            os: self,
            chunk_size: sz,
        }
    }

    pub fn mut_chunks_timeout(&mut self, sz: usize, t: Duration)
        -> OrderedChunksTimeout<T>
    {
        OrderedChunksTimeout {
            os: self,
            chunk_size: sz,
            timeout: t,
        }
    }

    pub fn iter_mut(&mut self)
        -> OrderedIter<T>
    {
        OrderedIter {
            os: self,
        }
    }

    pub fn iter_mut_timeout(&mut self, t: Duration)
        -> OrderedIterTimeout<T>
    {
        OrderedIterTimeout {
            os: self,
            timeout: t,
        }
    }

    pub fn enumerate(&mut self)
        -> OrderedIterEnumerated<T>
    {
        OrderedIterEnumerated {
            os: self,
        }
    }

    pub fn enumerate_timeout(&mut self, t: Duration)
        -> OrderedIterTimeoutEnumerated<T>
    {
        OrderedIterTimeoutEnumerated {
            os: self,
            timeout: t,
        }
    }
}

pub struct OrderedChunks<'a, T: Clone> {
    os: &'a mut OrderedStream<T>,
    chunk_size: usize,
}

impl<'a, T: Clone> Iterator for OrderedChunks<'a, T> {
    type Item = Vec<T>;

    fn next(&mut self) -> Option<Vec<T>> {
        match self.os.squeeze(self.chunk_size) {
            Ok(msg) => return Some(msg),
            Err(_)  => return None,
        }
    }
}

pub struct OrderedChunksTimeout<'a, T: Clone> {
    os: &'a mut OrderedStream<T>,
    chunk_size: usize,
    timeout: Duration,
}

impl<'a, T: Clone> Iterator for OrderedChunksTimeout<'a, T> {
    type Item = Vec<T>;

    fn next(&mut self) -> Option<Vec<T>> {
        match self.os.squeeze_timeout(self.chunk_size, self.timeout) {
            Ok(msg) => return Some(msg),
            Err(_)  => return None,
        }
    }
}

pub struct OrderedIter<'a, T: Clone> {
    os: &'a mut OrderedStream<T>,
}

impl<'a, T: Clone> Iterator for OrderedIter<'a, T> {
    type Item = Vec<T>;

    fn next(&mut self) -> Option<Vec<T>> {
        match self.os.current() {
            Ok(msg) => return Some(msg),
            Err(_)  => return None,
        }
    }
}

pub struct OrderedIterTimeout<'a, T: Clone> {
    os: &'a mut OrderedStream<T>,
    timeout: Duration,
}

impl<'a, T: Clone> Iterator for OrderedIterTimeout<'a, T> {
    type Item = Vec<T>;

    fn next(&mut self) -> Option<Vec<T>> {
        match self.os.current_timeout(self.timeout) {
            Ok(msg) => return Some(msg),
            Err(_)  => return None,
        }
    }
}

pub struct OrderedIterEnumerated<'a, T: Clone> {
    os: &'a mut OrderedStream<T>,
}

impl<'a, T: Clone> Iterator for OrderedIterEnumerated<'a, T> {
    type Item = (u64, Vec<T>);

    fn next(&mut self) -> Option<(u64, Vec<T>)> {
        let i = self.os.pos();
        match self.os.current() {
            Ok(msg) => return Some((i, msg)),
            Err(_)  => return None,
        }
    }
}

pub struct OrderedIterTimeoutEnumerated<'a, T: Clone> {
    os: &'a mut OrderedStream<T>,
    timeout: Duration,
}

impl<'a, T: Clone> Iterator for OrderedIterTimeoutEnumerated<'a, T> {
    type Item = (u64, Vec<T>);

    fn next(&mut self) -> Option<(u64, Vec<T>)> {
        let i = self.os.pos();
        match self.os.current_timeout(self.timeout) {
            Ok(msg) => return Some((i, msg)),
            Err(_)  => return None,
        }
    }
}

#[cfg(test)]
mod tests {
    use crossbeam_channel::unbounded;
    use std::str;
    use super::*;

    #[test]
    fn test_all() {
        abc();
        lgmsg_smblock();
        smmsg_lgblock();
        msg_iter();
        abc_blocks();
        iteration();
        chunking();
        enumeriterate();
        strings();
        yewsixfours();
    }

    fn abc() {
        println!("\n\nalphabet test");
        let (tx, rx) = unbounded();
        let mut os = OrderedStream::with_capacity_recvr(64, rx);

        let mut seq = 25;
        for num in 0..26 {
            let v = vec![(num + 97), (num + 97)];
            match tx.send((seq as u64, v)) {
                Ok(_) => (),
                Err(_) => println!("err {:?}", num),
            }
            seq -= 1;
        }

        let mut last  = 0u8;
        let mut check = false;

        loop {
            let timer = Instant::now();
            let tmp = os.squeeze_timeout(1, Duration::from_millis(1));
            println!("squeezed 1 byte in {:#?} (1 ms timeout)", timer.elapsed());
            match tmp {
                Ok(x) => {
                    assert_eq!(x.len(), 1);
                    if check {
                        assert_eq!(last, x[0]);
                    }
                    last = x[0];
                    check = !check;
                },
                Err(e) => {
                    match e {
                        Some(error) => {
                            println!("{:?}", error);
                            break;
                        },
                        None => continue,
                    }
                },
            }
        }
    }

    fn abc_blocks() {
        println!("\n\nalphablock test");
        let (tx, rx) = unbounded();
        let mut os = OrderedStream::with_capacity_recvr(64, rx);

        let test = b"abcdefghijklmnopqrstuvwxyz";
        let should_one = b"abcdefghijklmnopqrstuvwxyzabcdefghijklm";
        let should_two = b"nopqrstuvwxyzabcdefghijklmnopqrstuvwxyz";
        assert_eq!(should_one.len(), 39);
        assert_eq!(should_two.len(), 39);
        let mut msg = Vec::with_capacity(26);
        msg.extend_from_slice(test);

        for num in 0..3 {
            match tx.send((num, msg.clone())) {
                Ok(_) => (),
                Err(_) => println!("err {:?}", num),
            }
        }
        let mut one = Vec::with_capacity(39);
        let mut two = Vec::with_capacity(39);
        let mut cnt = 0;
        loop {
            let timer = Instant::now();
            let tmp = os.squeeze_timeout(39, Duration::from_millis(1));
            println!("squeezed 39 bytes in {:#?} (1 ms timeout)", timer.elapsed());
            match tmp {
                Ok(x) => {
                    assert_eq!(x.len(), 39);
                    if cnt == 0 {
                        one.extend_from_slice(&x);
                    } else if cnt == 1 {
                        two.extend_from_slice(&x);
                    }
                },
                Err(error) => {
                    match error {
                        Some(e) => {
                            println!("{:?}", e);
                            break;
                        },
                        None => continue,
                    }
                },
            }
            cnt += 1;
        }
        assert_eq!(one[..], should_one[..]);
        assert_eq!(two[..], should_two[..]);
    }

    fn lgmsg_smblock() {
        println!("\n\nsending large messages and squeezing small blocks");
        let (tx, rx) = unbounded();
        let mut os = OrderedStream::with_capacity_recvr(4, rx);

        let msg = vec![97u8; 32*1024*1024];

        for i in 0u64..4 {
            match tx.send((i, msg.clone())) {
                Ok(_) => (),
                Err(e) => println!("{:?}", e),
            }
            //println!("sending {} bytes for the {} time", msg.len(), i);
        }

        loop {
            let timer = Instant::now();
            let tmp = os.squeeze_timeout(7*1024*1024, Duration::from_millis(4));
            match tmp {
                Ok(x) => {
                    assert!(x.len() <= 7*1024*1024);
                    println!("squeezed {} mib in {:#?} (4 ms timeout)", (x.len() / 1024)/1024, timer.elapsed());
                },
                Err(error) => {
                    match error {
                        Some(_) => break,
                        None => continue,
                    }
                },
            }
        }
    }

    fn smmsg_lgblock() {
        println!("\n\nsending small messages and squeezing large blocks");
        let (tx, rx) = unbounded();
        let mut os = OrderedStream::with_capacity_recvr(64*1024, rx);

        let msg = vec![97u8; 1350];

        for i in 0u64..64*1024 {
            match tx.send((i, msg.clone())) {
                Ok(_) => (),
                Err(e) => println!("{:?}", e),
            }
            //println!("sending {} bytes for the {} time", msg.len(), i);
        }

        loop {
            let timer = Instant::now();
            let tmp = os.squeeze_timeout(32*1024*1024, Duration::from_millis(4));
            match tmp {
                Ok(x) => {
                    assert!(x.len() <= 32*1024*1024);
                    println!("squeezed {} mib in {:#?} (4 ms timeout)", (x.len() / 1024)/1024, timer.elapsed());
                },
                Err(error) => {
                    match error{
                        Some(_) => break,
                        None => continue,
                    }
                },
            }
        }
    }

    fn msg_iter() {
        println!("\n\ngoing through messages in order");
        let (tx, rx) = unbounded();
        let mut os = OrderedStream::with_capacity_recvr(16, rx);

        let msg = vec![97u8; 1350];

        for i in 0u64..16 {
            match tx.send((i, msg.clone())) {
                Ok(_) => (),
                Err(e) => println!("{:?}", e),
            }
            //println!("sending {} bytes for the {} time", msg.len(), i);
        }

        loop {
            let timer = Instant::now();
            let tmp = os.current_timeout(Duration::from_micros(3));
            println!("got message in {:#?} (3 µs timeout)", timer.elapsed());
            match tmp {
                Ok(x) => assert_eq!(x.len(), 1350),
                Err(e) => {
                    match e {
                        Some(_) => break,
                        None => continue,
                    }
                },
            }
        }
    }

    fn iteration() {
        println!("\n\niterating through messages in order");
        let (tx, rx) = unbounded();
        let mut os = OrderedStream::with_capacity_recvr(16, rx);

        let msg = vec![97u8; 1350];

        for i in 0u64..16 {
            match tx.send((i, msg.clone())) {
                Ok(_) => (),
                Err(e) => println!("{:?}", e),
            }
            //println!("sending {} bytes for the {} time", msg.len(), i);
        }
        drop(tx);

        // this will enumerate items for this call only, not taking into account timeout interruptions
        os.iter_mut().enumerate().for_each(|msg| {
            assert_eq!(msg.1.len(), 1350);
            println!("{}, {}, len: {}", msg.0, str::from_utf8(&msg.1[..2]).unwrap_or("no"), msg.1.len());
        });
    }

    fn enumeriterate() {
        println!("\n\nenumerating messages in order");
        let (tx, rx) = unbounded();
        let mut os = OrderedStream::with_capacity_recvr(16, rx);

        let msg = vec![97u8; 1350];

        for i in 0u64..16 {
            match tx.send((i, msg.clone())) {
                Ok(_) => (),
                Err(e) => println!("{:?}", e),
            }
        }
        drop(tx);

        // this will return tuples containing the message index relative to the overall stream
        os.enumerate().for_each(|msg| {
            assert_eq!(msg.1.len(), 1350);
            println!("{}, {}, len: {}", msg.0, str::from_utf8(&msg.1[..2]).unwrap_or("no"), msg.1.len());
        });
    }

    fn chunking() {
        println!("\n\nchunking messages in order");
        let (tx, rx) = unbounded();
        let mut os = OrderedStream::with_capacity_recvr(16, rx);

        let msg = vec![97u8; 1350];

        for i in 0u64..16 {
            match tx.send((i, msg.clone())) {
                Ok(_) => (),
                Err(e) => println!("{:?}", e),
            }
        }
        drop(tx);
        os.mut_chunks(1024).enumerate().for_each(|msg| {
            assert!(msg.1.len() <= 1024);
            println!("{}, {}, len: {}", msg.0, str::from_utf8(&msg.1[..2]).unwrap_or("no"), msg.1.len());
        });
    }

    fn strings() {
        println!("\n\nstring test");
        let msg_one: Vec<char> = "hello ".to_string().chars().collect();
        let msg_two: Vec<char> = "there".to_string().chars().collect();

        let mut b = true;

        let (tx, rx) = unbounded();
        let mut os: OrderedStream<char> = OrderedStream::with_capacity_recvr(128, rx);

        for i in 0..100 {
            match b {
                true  => tx.send((i, msg_one.clone())).expect("no"),
                false => tx.send((i, msg_two.clone())).expect("no"),
            }
            b = !b;
        }
        drop(tx);

        let test_against = "hello therehello there";
        for msg in os.mut_chunks(22) {
            let m: String = msg.iter().collect();
            assert_eq!(&m, test_against);
            println!("msg: {}", m);
        }
    }

    fn yewsixfours() {
        println!("\n\nu64 test");
        let msg_one: Vec<u64> = vec![1,2,3,4,5];
        let msg_two: Vec<u64> = vec![6,7,8,9,10];

        let mut b = true;

        let (tx, rx) = unbounded();
        let mut os: OrderedStream<u64> = OrderedStream::with_capacity_recvr(128, rx);

        for i in 0..100 {
            match b {
                true  => tx.send((i, msg_one.clone())).expect("no"),
                false => tx.send((i, msg_two.clone())).expect("no"),
            }
            b = !b;
        }
        drop(tx);

        let test_against = vec![1,2,3,4,5,6,7,8,9,10];
        for msg in os.mut_chunks(10) {
            assert_eq!(msg, test_against);
            println!("msg: {:?}", msg);
        }
    }
}
