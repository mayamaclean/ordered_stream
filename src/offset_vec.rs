use std::ops::{Index, IndexMut, RangeTo, RangeFrom, RangeFull, Range};

pub struct OffsetVec<T> {
    data: Vec<T>,
    offset: usize,
}

impl<T: Clone> Default for OffsetVec<T> {
    fn default()
        -> OffsetVec<T>
    {
        OffsetVec {
            data: Vec::new(),
            offset: 0,
        }
    }
}

impl<T: Clone> OffsetVec<T> {
    pub fn from_pieces(d: Vec<T>, o: usize)
        -> OffsetVec<T>
    {
        OffsetVec {
            data: d,
            offset: o,
        }
    }

    pub fn from_vec(v: Vec<T>)
        -> OffsetVec<T>
    {
        OffsetVec {
            data: v,
            offset: 0,
        }
    }

    pub fn with_capacity(cap: usize)
        -> OffsetVec<T>
    {
        let temp: Vec<T> = Vec::with_capacity(cap);

        OffsetVec {
            data: temp,
            offset: 0,
        }
    }

    pub fn new()
        -> OffsetVec<T>
    {
        Default::default()
    }

    pub fn len(&self)
        -> usize
    {
        self.data.len() - self.offset
    }

    pub fn offset(&self)
        -> usize
    {
        self.offset
    }

    pub fn inc_offset(&mut self, o: usize)
        -> Result<(),()>
    {
        let new_offset = self.offset + o;
        if new_offset >= self.data.len() {
            return Err(());
        }
        self.offset = new_offset;
        Ok(())
    }

    pub fn into_inner(self)
        -> Vec<T>
    {
        self.data
    }

    pub fn is_empty(&self)
        -> bool
    {
        self.data.is_empty()
    }

}

impl<T> Index<RangeFrom<usize>> for OffsetVec<T> {
    type Output = [T];

    #[inline]
    fn index(&self, idx: RangeFrom<usize>) -> &[T] {
        &self.data[(idx.start + self.offset)..]
    }
}

impl<T> Index<RangeTo<usize>> for OffsetVec<T> {
    type Output = [T];

    #[inline]
    fn index(&self, idx: RangeTo<usize>) -> &[T] {
        &self.data[self.offset..idx.end]
    }
}

impl<T> Index<RangeFull> for OffsetVec<T> {
    type Output = [T];

    #[inline]
    fn index(&self, _: RangeFull) -> &[T] {
        &self.data[self.offset..]
    }
}

impl<T> Index<Range<usize>> for OffsetVec<T> {
    type Output = [T];

    #[inline]
    fn index(&self, idx: Range<usize>) -> &[T] {
        &self.data[(self.offset + idx.start)..idx.end]
    }
}

impl<T> IndexMut<RangeFrom<usize>> for OffsetVec<T> {
    #[inline]
    fn index_mut<'a>(&'a mut self, idx: RangeFrom<usize>) -> &'a mut [T] {
        self.data.index_mut((idx.start + self.offset)..)
    }
}

impl<T> IndexMut<RangeTo<usize>> for OffsetVec<T> {
    #[inline]
    fn index_mut<'a>(&'a mut self, idx: RangeTo<usize>) -> &'a mut [T] {
        self.data.index_mut(self.offset..idx.end)
    }
}

impl<T> IndexMut<RangeFull> for OffsetVec<T> {
    #[inline]
    fn index_mut(&mut self, _: RangeFull) -> &mut [T] {
        self.data.index_mut(self.offset..)
    }
}

impl<T> IndexMut<Range<usize>> for OffsetVec<T> {
    #[inline]
    fn index_mut<'a>(&'a mut self, idx: Range<usize>) -> &'a mut [T] {
        self.data.index_mut((self.offset + idx.start)..idx.end)
    }
}

#[cfg(test)]
mod tests {
    use std::str;
    use super::*;

    #[test]
    fn test() {
        let mut alpha = Vec::with_capacity(26);
        for i in 0u8..26 {
            alpha.push(i + 97);
        }
        let mut test = OffsetVec::from_vec(alpha.clone());
        test.inc_offset(13)
            .expect("no");
        assert_eq!(test[..], alpha[13..]);
        println!("test: {}, alpha: {}", str::from_utf8(&test[..]).expect("no"), str::from_utf8(&alpha[13..]).expect("no"));
    }
}
