extern crate crossbeam_channel;
extern crate intmap;

use crossbeam_channel::{RecvError, RecvTimeoutError, Receiver, TryRecvError};
use intmap::IntMap;
use std::time::{Duration, Instant};

static BENCH: bool = true;

pub struct OrderedStream {
    pub lookup:   IntMap<Vec<u8>>,
    curr:     u64,
    incoming: Receiver<(u64, Vec<u8>)>,
}

impl OrderedStream {
    pub fn with_capacity_and_recvr(cap: usize, rx: Receiver<(u64, Vec<u8>)>)
        -> OrderedStream
    {
        OrderedStream {
            lookup: IntMap::with_capacity(cap),
            curr: 0,
            incoming: rx,
        }
    }

    pub fn with_recvr(rx: Receiver<(u64, Vec<u8>)>)
        -> OrderedStream
    {
        OrderedStream::with_capacity_and_recvr(16, rx)
    }

    pub fn set_recv(&mut self, rx: Receiver<(u64, Vec<u8>)>)
    {
        self.incoming = rx;
    }

    pub fn add_item_sep(&mut self, seq: u64, item: Vec<u8>)
        -> bool
    {
        self.lookup.insert(seq, item)
    }

    pub fn add_item(&mut self, msg: (u64, Vec<u8>))
        -> bool
    {
        self.add_item_sep(msg.0, msg.1)
    }

    pub fn chk_lookup(&mut self, seq: u64)
        -> bool
    {
        self.lookup.contains_key(seq)
    }

    pub fn rm_item(&mut self, seq: u64)
        -> Option<Vec<u8>>
    {
        self.lookup.remove(seq)
    }

    pub fn read_to_pos(&mut self, p: u64)
        -> Result<bool, RecvError>
    {
        if p < self.curr
        { return Ok(false); }

        if self.chk_lookup(p)
        { return Ok(false); }

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
        let i = self.curr;
        self.read_to_pos(i)
    }

    pub fn read_to_pos_timeout(&mut self, p: u64, d: Duration)
        -> Result<bool, RecvTimeoutError>
    {
        if p < self.curr { return Ok(false); }
        if self.chk_lookup(p) { return Ok(false); }

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
        let i = self.curr;
        self.read_to_pos_timeout(i, d)
    }

    pub fn read_msgs(&mut self, cnt: usize)
        -> (usize, Option<RecvError>)
    {
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

    pub fn squeeze(&mut self, len: usize)
        -> (Option<Vec<u8>>, Option<RecvError>)
    {
        let timer = Instant::now();

        let mut buf: Vec<u8>
            = Vec::with_capacity(len);

        let mut remainder
            = len;

        let mut tempcur
            = self.curr;

        match self
             .rm_item(tempcur)
        {
            Some(mut data) => {
                if data.len() == remainder {
                    self.curr += 1;
                    if BENCH {
                        println!("{:#?}", timer.elapsed());
                    }
                    return (Some(data), None);
                }
                else if data.len() > remainder {
                    let tmp = data.drain(0..remainder).collect();
                    self.add_item_sep(tempcur, data);

                    if BENCH {
                        println!("{:#?}", timer.elapsed());
                    }

                    return (Some(tmp), None);
                }
                remainder -= data.len();
                buf.append(&mut data);

                self.curr += 1;
                tempcur += 1;
            },
            None => {
                let e = self.read_to_seq_pos();

                match e {
                    Ok(_) => (),
                    Err(err) => {
                        return (None, Some(err));
                    },
                }
            },
        }

        loop {
            match self
                 .rm_item(tempcur)
            {
                Some(mut data) => {
                    if data.len() == remainder {
                        self.curr += 1;

                        buf.append(&mut data);

                        if BENCH {
                            println!("{:#?}", timer.elapsed());
                        }
                        return (Some(buf), None);
                    }
                    else if data.len() > remainder {
                        let reinsert: Vec<u8> = data.drain(remainder..).collect();

                        buf.append(&mut data);
                        self.add_item_sep(tempcur, reinsert);

                        if BENCH {
                            println!("{:#?}", timer.elapsed());
                        }
                        return (Some(buf), None);
                    }

                    remainder -= data.len();
                    buf.append(&mut data);
                    self.curr += 1;
                    tempcur += 1;
                },
                None => {
                    let e = self.read_to_seq_pos();

                    match e {
                        Ok(_) => (),
                        Err(err) => {
                            if buf.len() == 0 {
                                if BENCH {
                                    println!("{:#?}", timer.elapsed());
                                }
                                return (None, Some(err));
                            }
                            if BENCH {
                                println!("{:#?}", timer.elapsed());
                            }
                            return (Some(buf), Some(err));
                        }
                    }
                },
            }
        }
    }

    pub fn squeeze_timeout(&mut self, len: usize, d: Duration)
        -> (Option<Vec<u8>>, Option<RecvTimeoutError>)
    {
        let timer = Instant::now();

        let mut buf: Vec<u8>
            = Vec::with_capacity(len);

        let mut remainder
            = len;

        let mut tempcur
            = self.curr;

        match self
             .rm_item(tempcur)
        {
            Some(mut data) => {
                if data.len() == remainder {
                    self.curr += 1;
                    if BENCH {
                        println!("{:#?}", timer.elapsed());
                    }
                    return (Some(data), None);
                }
                else if data.len() > remainder {
                    let tmp = data.drain(0..remainder).collect();

                    self.add_item_sep(tempcur, data);

                    if BENCH {
                        println!("{:#?}", timer.elapsed());
                    }
                    return (Some(tmp), None);
                }
                remainder -= data.len();
                buf.append(&mut data);
                self.curr += 1;
                tempcur += 1;
            },
            None => {
                let e = self.read_to_seq_pos_timeout(d);

                match e {
                    Ok(_) => (),
                    Err(err) => {
                        if BENCH {
                            println!("{:#?}", timer.elapsed());
                        }
                        return (None, Some(err));
                    },
                }
            },
        }

        loop {
            match self
                 .rm_item(tempcur)
            {
                Some(mut data) => {
                    if data.len() == remainder {
                        self.curr += 1;
                        buf.append(&mut data);

                        if BENCH {
                            println!("{:#?}", timer.elapsed());
                        }
                        return (Some(buf), None);
                    }
                    else if data.len() > remainder {
                        let reinsert: Vec<u8> = data.drain(remainder..).collect();

                        buf.append(&mut data);
                        self.add_item_sep(tempcur, reinsert);

                        if BENCH {
                            println!("{:#?}", timer.elapsed());
                        }
                        return (Some(buf), None);
                    }
                    remainder -= data.len();
                    buf.append(&mut data);
                    self.curr += 1;
                    tempcur += 1;
                },
                None => {
                    let e = self.read_to_seq_pos_timeout(d);

                    match e {
                        Ok(_) => (),
                        Err(err) => {
                            if buf.len() == 0 {
                                if BENCH {
                                    println!("{:#?}", timer.elapsed());
                                }
                                return (None, Some(err));
                            }
                            if BENCH {
                                println!("{:#?}", timer.elapsed());
                            }
                            return (Some(buf), Some(err));
                        }
                    }
                },
            }
        }
    }

    pub fn current(&mut self)
        -> (Option<Vec<u8>>, Option<RecvError>)
    {
        let timer = Instant::now();
        let i = self
               .curr
               .clone();

        match self
             .rm_item(i)
        {
            Some(item) => {
                self.curr += 1;
                if BENCH {
                    println!("{:#?}", timer.elapsed());
                }
                return (Some(item), None);
            },
            None => {
                match self.read_to_seq_pos() {
                    Ok(b) => {
                        if b == true {
                            self.curr += 1;
                            if BENCH {
                                println!("{:#?}", timer.elapsed());
                            }
                            return (self.rm_item(i), None);
                        }
                        if BENCH {
                            println!("{:#?}", timer.elapsed());
                        }
                        return (None, None);
                    },
                    Err(e) => {
                        if BENCH {
                            println!("{:#?}", timer.elapsed());
                        }
                        return (None, Some(e));
                    },
                }
            }
        }
    }

    pub fn current_timeout(&mut self, d: Duration)
        -> (Option<Vec<u8>>, Option<RecvTimeoutError>)
    {
        let timer = Instant::now();
        let i = self
               .curr
               .clone();

        match self
             .rm_item(i)
        {
            Some(item) => {
                self.curr += 1;
                if BENCH {
                    println!("{:#?}", timer.elapsed());
                }
                return (Some(item), None);
            },
            None => {
                match self
                     .read_to_seq_pos_timeout(d)
                {
                    Ok(b) => {
                        if b == true {
                            self.curr += 1;
                            if BENCH {
                                println!("{:#?}", timer.elapsed());
                            }
                            return (self.rm_item(i), None);
                        }
                        if BENCH {
                            println!("{:#?}", timer.elapsed());
                        }
                        return (None, None);
                    },
                    Err(e) => {
                        if BENCH {
                            println!("{:#?}", timer.elapsed());
                        }
                        return (None, Some(e));
                    },
                }
            }
        }
    }

    pub fn position(&self)
        -> u64
    {
        self.curr
    }

    pub fn sop(&mut self)
        -> Result<usize, TryRecvError>
    {
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
}

#[cfg(test)]
mod tests {
    use crossbeam_channel::unbounded;
    use std::io::{self, Read};
    use std::str;
    use std::thread;
    use super::*;

    #[test]
    fn test_all() {
        abc();
        lgmsg_smblock();
        smmsg_lgblock();
        msg_iter();
    }

    fn abc() {
        println!("alphabet test");
        let (tx, rx) = unbounded();
        let mut os = OrderedStream::with_capacity_and_recvr(64, rx);

        let mut seq = 25;
        for num in 0..26 {
            match tx.send((seq as u64, vec![(num + 97), (num + 97)])) {
                Ok(_) => (),
                Err(e) => println!("err {:?}", num),
            }
            seq -= 1;
        }

        let mut iter  = 0;
        let mut last  = 0u8;
        let mut check = false;

        loop {
            let tmp = os.squeeze_timeout(1, Duration::from_millis(1));
            match tmp.0 {
                Some(x) => {
                    println!("iter: {}, data: {}", iter, str::from_utf8(&x).expect("utf error"));
                    if check {
                        println!("asserting {} == {}", last, x[0]);
                        assert_eq!(last, x[0]);
                    }
                    last = x[0];
                    check = !check;
                },
                None => {
                    match tmp.1 {
                        Some(e) => {
                            println!("{:?}", e);
                            break;
                        },
                        None => continue,
                    }
                },
            }
            iter += 1;
        }
    }

    fn lgmsg_smblock() {
        println!("sending large messages and squeezing small blocks");
        let (tx, rx) = unbounded();
        let mut os = OrderedStream::with_capacity_and_recvr(4, rx);

        let msg = vec![97u8; 8*1024*1024];

        for i in 0u64..4 {
            match tx.send((i, msg.clone())) {
                Ok(_) => (),
                Err(e) => println!("{:?}", e),
            }
            //println!("sending {} bytes for the {} time", msg.len(), i);
        }

        let mut iter = 0;

        loop {
            println!("lookup len: {}", os.lookup.len());
            let tmp = os.squeeze_timeout(8*1024, Duration::from_millis(4));
            match tmp.0 {
                Some(x) => println!("iter: {}, len: {}\ndata: {}", iter, x.len(), str::from_utf8(&x[0..16]).expect("utf error")),
                None => {
                    match tmp.1 {
                        Some(e) => break,
                        None => continue,
                    }
                },
            }
            iter += 1;
        }
    }

    fn smmsg_lgblock() {
        println!("sending small messages and squeezing large blocks");
        let (tx, rx) = unbounded();
        let mut os = OrderedStream::with_capacity_and_recvr(8196, rx);

        let msg = vec![97u8; 1350];

        for i in 0u64..8196 {
            match tx.send((i, msg.clone())) {
                Ok(_) => (),
                Err(e) => println!("{:?}", e),
            }
            //println!("sending {} bytes for the {} time", msg.len(), i);
        }

        let mut iter = 0;

        loop {
            println!("lookup len: {}", os.lookup.len());
            let tmp = os.squeeze_timeout(8*1024*1024, Duration::from_millis(4));
            match tmp.0 {
                Some(x) => println!("iter: {}, len: {}\ndata: {}", iter, x.len(), str::from_utf8(&x[0..16]).expect("utf error")),
                None => {
                    match tmp.1 {
                        Some(e) => break,
                        None => continue,
                    }
                },
            }
            iter += 1;
        }
    }

    fn msg_iter() {
        println!("going through messages in order");
        let (tx, rx) = unbounded();
        let mut os = OrderedStream::with_capacity_and_recvr(16, rx);

        let msg = vec![97u8; 1350];

        for i in 0u64..16 {
            match tx.send((i, msg.clone())) {
                Ok(_) => (),
                Err(e) => println!("{:?}", e),
            }
            //println!("sending {} bytes for the {} time", msg.len(), i);
        }

        let mut iter = 0;

        loop {
            println!("lookup len: {}", os.lookup.len());
            let tmp = os.current_timeout(Duration::from_millis(1));
            match tmp.0 {
                Some(x) => println!("iter: {}, len: {}\ndata: {}", iter, x.len(), str::from_utf8(&x[0..16]).expect("utf error")),
                None => {
                    match tmp.1 {
                        Some(e) => break,
                        None => continue,
                    }
                },
            }
            iter += 1;
        }
    }
}
