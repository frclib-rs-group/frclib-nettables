

use nohash_hasher::IsEnabled;


#[derive(Debug, Clone, Default)]
pub struct PrimBiMap<L, R> {
    pub(crate) map: nohash_hasher::IntMap<L, R>,
    pub(crate) rev_map: nohash_hasher::IntMap<R, L>,
}

#[allow(dead_code)]
impl<L, R> PrimBiMap<L, R> 
where
    L: std::hash::Hash + Eq + Copy + IsEnabled,
    R: std::hash::Hash + Eq + Copy + IsEnabled,
{
    pub(crate) fn new() -> Self {
        Self {
            map: nohash_hasher::IntMap::default(),
            rev_map: nohash_hasher::IntMap::default(),
        }
    }

    #[allow(clippy::default_trait_access)]
    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self {
            map: nohash_hasher::IntMap::with_capacity_and_hasher(capacity, Default::default()),
            rev_map: nohash_hasher::IntMap::with_capacity_and_hasher(capacity, Default::default()),
        }
    }

    pub(crate) fn insert(&mut self, left: L, right: R) -> (Option<L>, Option<R>) {
        let l = self.rev_map.insert(right, left);
        let r = self.map.insert(left, right);
        (l, r)
    }

    pub(crate) fn get_by_right(&self, right: &R) -> Option<&L> {
        self.rev_map.get(right)
    }

    pub(crate) fn get_by_left(&self, left: &L) -> Option<&R> {
        self.map.get(left)
    }

    pub(crate) fn remove_by_left(&mut self, left: &L) -> Option<R> {
        self.map.remove(left)
    }

    pub(crate) fn remove_by_right(&mut self, right: &R) -> Option<L> {
        self.rev_map.remove(right)
    }

    pub(crate) fn contains_left(&self, left: &L) -> bool {
        self.map.contains_key(left)
    }

    pub(crate) fn contains_right(&self, right: &R) -> bool {
        self.rev_map.contains_key(right)
    }

    pub(crate) fn len(&self) -> usize {
        self.map.len()
    }

    pub(crate) fn iter_lr(&self) -> impl Iterator<Item = (&L, &R)> {
        self.map.iter()
    }

    pub(crate) fn iter_rl(&self) -> impl Iterator<Item = (&R, &L)> {
        self.rev_map.iter()
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn test_bimap() {
        let mut map = super::PrimBiMap::new();
        map.insert(1, 2);

        assert_eq!(map.get_by_left(&1), Some(&2));
        assert_eq!(map.get_by_right(&2), Some(&1));
    }

    #[test]
    fn test_bimap_remove() {
        let mut map = super::PrimBiMap::new();
        map.insert(1, 2);

        assert_eq!(map.remove_by_left(&1), Some(2));
        assert_eq!(map.remove_by_right(&2), Some(1));
    }
}