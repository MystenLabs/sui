// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use rand::Rng;

pub fn checksum(item: &[u32; 4]) -> u32 {
    item[0].rotate_left(3)
        ^ item[1].rotate_left(7)
        ^ item[2].rotate_left(11)
        ^ item[3].rotate_left(17)
}

#[derive(Default, Clone, PartialEq, Eq, Debug)]
pub struct IbltEntry {
    pub count: u32,
    pub item: [u32; 4],
    pub checksum: u32,
}

impl IbltEntry {
    pub fn new(bytes: [u8; 16]) -> IbltEntry {
        let item = [
            u32::from_be_bytes(bytes[0..4].try_into().unwrap()),
            u32::from_be_bytes(bytes[4..8].try_into().unwrap()),
            u32::from_be_bytes(bytes[8..12].try_into().unwrap()),
            u32::from_be_bytes(bytes[12..16].try_into().unwrap()),
        ];
        let checksum = checksum(&item);

        IbltEntry {
            count: 1,
            item,
            checksum,
        }
    }

    pub fn random() -> IbltEntry {
        let mut rng = rand::thread_rng();

        let item: [u32; 4] = [rng.gen(), rng.gen(), rng.gen(), rng.gen()];
        let checksum = checksum(&item);

        IbltEntry {
            count: 1,
            item,
            checksum,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub fn checksum_ok(&self) -> bool {
        self.checksum == checksum(&self.item)
    }

    pub fn positions(&self) -> [u32; 4] {
        [
            // TODO: here we can afford to make sure they are different.
            self.item[0].wrapping_add(self.checksum).wrapping_add(128),
            self.item[1].wrapping_add(self.checksum).wrapping_add(32),
            self.item[2].wrapping_add(self.checksum).wrapping_add(16),
            self.item[3].wrapping_add(self.checksum).wrapping_add(8),
        ]
    }

    pub fn add(&mut self, other: &IbltEntry) {
        self.count = self.count.wrapping_add(other.count);
        self.item[0] = self.item[0].wrapping_add(other.item[0]);
        self.item[1] = self.item[1].wrapping_add(other.item[1]);
        self.item[2] = self.item[2].wrapping_add(other.item[2]);
        self.item[3] = self.item[3].wrapping_add(other.item[3]);
        self.checksum = self.checksum.wrapping_add(other.checksum);
    }

    pub fn sub(&mut self, other: &IbltEntry) {
        self.count = self.count.wrapping_sub(other.count);
        self.item[0] = self.item[0].wrapping_sub(other.item[0]);
        self.item[1] = self.item[1].wrapping_sub(other.item[1]);
        self.item[2] = self.item[2].wrapping_sub(other.item[2]);
        self.item[3] = self.item[3].wrapping_sub(other.item[3]);
        self.checksum = self.checksum.wrapping_sub(other.checksum);
    }

    pub fn neg(&mut self) {
        self.count = self.count.wrapping_neg();
        self.item[0] = self.item[0].wrapping_neg();
        self.item[1] = self.item[1].wrapping_neg();
        self.item[2] = self.item[2].wrapping_neg();
        self.item[3] = self.item[3].wrapping_neg();
        self.checksum = self.checksum.wrapping_neg();
    }

    pub fn extract(&self) -> Option<(IbltEntry, bool)> {
        // Positive case
        if self.count == 1 && self.checksum_ok() {
            return Some((self.clone(), true));
        }

        // Negative case
        if self.count == 1u32.wrapping_neg() {
            let mut c = self.clone();
            c.neg();
            if c.checksum_ok() {
                return Some((c, false));
            }
        }
        None
    }
}

#[allow(dead_code)]
pub struct IbltFilter {
    pub base_size: u64,
    pub level: u64,
    elements: Vec<IbltEntry>,
}

impl IbltFilter {
    pub fn new(base_size: u64, level: u64) -> IbltFilter {
        let size = base_size * (2u64.pow(level as u32));
        IbltFilter {
            base_size,
            level,
            elements: vec![IbltEntry::default(); size as usize],
        }
    }

    pub fn is_empty(&self) -> bool {
        self.elements.iter().all(IbltEntry::is_empty)
    }

    pub fn add(&mut self, item: &IbltEntry) {
        let size = self.elements.len();
        let [pos1, pos2, pos3, pos4] = item.positions();
        self.elements[pos1 as usize % size].add(item);
        self.elements[pos2 as usize % size].add(item);
        self.elements[pos3 as usize % size].add(item);
        self.elements[pos4 as usize % size].add(item);
    }

    pub fn diff(&mut self, other: &IbltFilter) {
        assert!(self.elements.len() == other.elements.len());

        for (item, other_item) in (&mut self.elements).iter_mut().zip(&other.elements) {
            item.sub(other_item)
        }
    }

    pub fn decode(&mut self) -> Vec<(IbltEntry, bool)> {
        let mut extracted_items = Vec::new();
        let size = self.elements.len();

        loop {
            let mut progress = 0;
            for i in 0..self.elements.len() {
                if let Some((item, direction)) = self.elements[i].extract() {
                    progress += 1;
                    let [pos1, pos2, pos3, pos4] = item.positions();
                    if direction {
                        // positive direction, need to subtract
                        self.elements[pos1 as usize % size].sub(&item);
                        self.elements[pos2 as usize % size].sub(&item);
                        self.elements[pos3 as usize % size].sub(&item);
                        self.elements[pos4 as usize % size].sub(&item);
                    } else {
                        // negative direction, need to add
                        self.elements[pos1 as usize % size].add(&item);
                        self.elements[pos2 as usize % size].add(&item);
                        self.elements[pos3 as usize % size].add(&item);
                        self.elements[pos4 as usize % size].add(&item);
                    }
                    extracted_items.push((item, direction));
                }
            }
            if progress == 0 {
                break;
            }
        }
        extracted_items
    }

    pub fn compress(&self, new_level: u64) -> IbltFilter {
        assert!(new_level < self.level);
        let mut new_filter = IbltFilter::new(self.base_size, new_level);
        for chunk in self.elements[..].chunks(new_filter.elements.len()) {
            new_filter
                .elements
                .iter_mut()
                .zip(chunk)
                .for_each(|(newe, olde)| newe.add(olde));
        }
        new_filter
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_iblt_entry() {
        let mut e1 = IbltEntry::new([1; 16]);
        assert!(IbltEntry::default() != e1);
        // Extract a single element works
        assert!(e1.extract().unwrap().0 == e1);

        // Take the negative of e1
        let mut e12 = e1.clone();
        e12.neg();
        // Extract a negative element works
        assert!(e12.extract().unwrap().0 == e1);

        // Add it back to e1
        e1.add(&e12);
        assert_eq!(IbltEntry::default(), e1);
    }

    #[test]
    fn test_iblt_sub() {
        let mut e1 = IbltEntry::new([1; 16]);
        assert!(IbltEntry::default() != e1);
        // Extract a single element works
        assert!(e1.extract().unwrap().0 == e1);

        // Take the negative of e1
        let e12 = e1.clone();

        // Add it back to e1
        e1.sub(&e12);
        assert_eq!(IbltEntry::default(), e1);
    }

    #[test]
    fn test_iblt_filter() {
        let e1 = IbltEntry::new([1; 16]);
        let e2 = IbltEntry::new([2; 16]);

        let mut f1 = IbltFilter::new(128, 4);
        f1.add(&e1);
        let mut f2 = IbltFilter::new(128, 4);
        f2.add(&e2);

        f1.diff(&f2);

        let x = f1.decode();
        assert!(x.len() == 2);
        assert!(f1.is_empty());
    }

    #[test]
    fn test_iblt_filter_many() {
        let mut f1 = IbltFilter::new(128, 4);
        let mut f2 = IbltFilter::new(128, 4);

        // Many in common
        for _ in 0..1000 {
            let e1 = IbltEntry::random();
            f1.add(&e1);
            f2.add(&e1);
        }

        // A few not in common
        for _ in 0..10 {
            let e1 = IbltEntry::random();
            let e2 = IbltEntry::random();
            f1.add(&e1);
            f2.add(&e2);
        }

        f1.diff(&f2);

        let x = f1.decode();
        assert!(x.len() == 20);
        assert!(f1.is_empty());
    }

    #[test]
    fn test_iblt_filter_many_compress() {
        let mut f1 = IbltFilter::new(128, 4);
        let mut f2 = IbltFilter::new(128, 4);

        // Many in common
        for _ in 0..1000 {
            let e1 = IbltEntry::random();
            f1.add(&e1);
            f2.add(&e1);
        }

        // A few not in common
        for _ in 0..10 {
            let e1 = IbltEntry::random();
            let e2 = IbltEntry::random();
            f1.add(&e1);
            f2.add(&e2);
        }

        f1 = f1.compress(0);
        f2 = f2.compress(0);

        f1.diff(&f2);

        let x = f1.decode();
        assert!(x.len() == 20);
        assert!(f1.is_empty());
    }
}
