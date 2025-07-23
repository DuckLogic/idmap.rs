//! Efficient maps of integer id keys to values, backed by an underlying `Vec`.
//!
//! However, unless a `CompactIdMap` is used, space requirements are O(n) the largest key.
//! Any type that implements `IntegerId` can be used for the key,
//! but no storage is wasted if the key can be represented from the id.
#![cfg_attr(feature = "nightly", feature(trusted_len, drain_filter))]
#![deny(missing_docs)]
extern crate fixedbitset;
#[cfg(feature = "petgraph")]
extern crate petgraph;
#[cfg(feature = "serde")]
extern crate serde;

use std::borrow::Borrow;
use std::fmt::{self, Debug, Formatter};
use std::iter::{self, FromIterator};
use std::marker::PhantomData;
use std::ops::{Index, IndexMut};

pub mod direct;
#[cfg(feature = "petgraph")]
mod graph;
mod integer_id;
pub mod ordered;
#[cfg(feature = "serde")]
mod serialization;
pub mod set;
pub mod table;
mod utils;

pub use integer_id::IntegerId;
pub use set::IdSet;
use table::{
    DenseEntryTable, DirectEntryTable, EntryIterable, EntryTable, SafeEntries, SafeEntriesMut,
};

pub use self::direct::DirectIdMap;

pub use self::ordered::OrderedIdMap;

/// A map of mostly-contiguous `IntegerId` keys to values, backed by a `Vec`.
///
/// This is parametric over the type of the underlying `EntryTable`, which controls its behavior
/// By default it's equivalent to an `OrderedIdMap`,
/// though you can explicitly request a `DirectIdMap` instead.
///
/// From the user's perspective, this is equivalent to a nice wrapper around a `Vec<Option<(K, V)>>`,
/// that preserves insertion order and saves some space for missing keys.
/// More details on the possible internal representations
/// are documented in the `OrderedIdMap` and `DirectIdMap` aliases.
pub struct IdMap<K: IntegerId, V, T: EntryTable<K, V> = DenseEntryTable<K, V>> {
    entries: T,
    marker: PhantomData<DirectEntryTable<K, V>>,
}
impl<K: IntegerId, V> IdMap<K, V, DirectEntryTable<K, V>> {
    /// Create a new direct IdMap.
    ///
    /// This stores its entries directly in a Vector
    #[inline]
    pub fn new_direct() -> Self {
        IdMap::new_other()
    }
    /// Create new direct IdMap, initialized with the specified capacity
    ///
    /// Because a direct id map stores its values directly,
    /// the capacity hints at the maximum id and not the length
    #[inline]
    pub fn with_capacity_direct(capacity: usize) -> Self {
        IdMap::with_capacity_other(capacity)
    }
}
impl<K: IntegerId, V> IdMap<K, V> {
    /// Create an empty IdMap using a `DenseEntryTable` and `OrderedIdTable`
    #[inline]
    pub fn new() -> Self {
        IdMap {
            entries: DenseEntryTable::new(),
            marker: PhantomData,
        }
    }
    /// Create an IdMap with the specified capacity, using an `OrderedIdTable`
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        IdMap {
            entries: DenseEntryTable::with_capacity(capacity),
            marker: PhantomData,
        }
    }
}
impl<K: IntegerId, V, T: EntryTable<K, V>> IdMap<K, V, T> {
    /// Create an empty `IdMap` with a custom entry table type.
    #[inline]
    pub fn new_other() -> Self {
        IdMap {
            entries: T::new(),
            marker: PhantomData,
        }
    }
    /// Create a new `IdMap` with the specified capacity but a custom entry table.
    #[inline]
    pub fn with_capacity_other(capacity: usize) -> Self {
        IdMap {
            entries: T::with_capacity(capacity),
            marker: PhantomData,
        }
    }
    /// If this map is empty
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    /// The length of this map
    ///
    /// This doesn't necessarily equal the maximum integer id
    #[inline]
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    /// The maximum id of all the elements in this map
    #[inline]
    pub fn max_id(&self) -> Option<u64> {
        self.entries.max_id()
    }
    /// If this map contains the specified key
    #[inline]
    pub fn contains_key<Q: Borrow<K>>(&self, key: Q) -> bool {
        self.get(key).is_some()
    }
    /// Retrieve the value associated with the specified key,
    /// or `None` if the value is not present
    ///
    /// Keys that implement `IntegerId` are expected to be cheap,
    /// so we don't borrow the key.
    #[inline]
    pub fn get<Q: Borrow<K>>(&self, key: Q) -> Option<&V> {
        let key = key.borrow();
        self.entries.get(key)
    }
    /// Retrieve a mutable reference to the value associated with the specified key,
    /// or `None` if the value is not present
    #[inline]
    pub fn get_mut<Q: Borrow<K>>(&mut self, key: Q) -> Option<&mut V> {
        let key = key.borrow();
        self.entries.get_mut(key)
    }
    /// Insert the specified value, associating it with a key
    ///
    /// Returns the value previously associated with they key,
    /// or `None` if there wasn't any
    #[inline]
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        self.entries.insert(key, value)
    }
    /// Remove the value associated with the specified key,
    /// or `None` if there isn't any
    #[inline]
    pub fn remove<Q: Borrow<K>>(&mut self, key: Q) -> Option<V> {
        let key = key.borrow();
        self.entries.swap_remove(key)
    }
    /// Retrieve the entry associated with the specified key
    ///
    /// Mimics the HashMap entry aPI
    #[inline]
    pub fn entry(&mut self, key: K) -> Entry<K, V, T> {
        if self.entries.get(&key).is_some() {
            Entry::Occupied(OccupiedEntry { map: self, key })
        } else {
            Entry::Vacant(VacantEntry { key, map: self })
        }
    }
    /// Iterate over all the keys and values in this map
    #[inline]
    pub fn iter(&self) -> Iter<K, V, T> {
        Iter(SafeEntries::new(&self.entries))
    }
    /// Iterate over all the entries in this map,
    /// giving mutable references to the keys
    #[inline]
    pub fn iter_mut(&mut self) -> IterMut<K, V, T> {
        IterMut(SafeEntriesMut::new(&mut self.entries))
    }
    /// Iterate over all the keys in the map
    #[inline]
    pub fn keys(&self) -> Keys<K, V, T> {
        Keys(SafeEntries::new(&self.entries))
    }
    /// Iterate over all the values in the map
    #[inline]
    pub fn values(&self) -> Values<K, V, T> {
        Values(SafeEntries::new(&self.entries))
    }
    /// Iterate over mutable references to all the values in the map
    #[inline]
    pub fn values_mut(&mut self) -> ValuesMut<K, V, T> {
        ValuesMut(SafeEntriesMut::new(&mut self.entries))
    }
    /// Clear all the entries the map
    ///
    /// Like [Vec::clear] this should retain the underlying memory
    /// for future use.
    #[inline]
    pub fn clear(&mut self) {
        self.entries.clear();
    }
    /// Retains only the elements specified by the predicate.
    /// ```
    /// # use idmap::IdMap;
    /// let mut map: IdMap<usize, usize> = (0..8).map(|x|(x, x*10)).collect();
    /// map.retain(|k, _| k % 2 == 0);
    /// assert_eq!(
    ///     map.into_iter().collect::<Vec<_>>(),
    ///     vec![(0, 0), (2, 20), (4, 40), (6, 60)]
    /// );
    /// ```
    #[inline]
    pub fn retain<F>(&mut self, func: F)
    where
        F: FnMut(&K, &mut V) -> bool,
    {
        self.entries.retain(func);
    }
    /// Reserve space for the specified number of additional elements
    #[inline]
    pub fn reserve(&mut self, amount: usize) {
        self.entries.reserve(amount);
    }
    /// Give a wrapper that will debug the underlying representation of this `IdMap`
    #[inline]
    pub fn raw_debug(&self) -> RawDebug<K, V, T>
    where
        K: Debug,
        V: Debug,
    {
        RawDebug(self)
    }
}
impl<K, V, T> Clone for IdMap<K, V, T>
where
    K: IntegerId + Clone,
    V: Clone,
    T: EntryTable<K, V>,
{
    #[inline]
    fn clone(&self) -> Self {
        IdMap {
            entries: self.entries.cloned(),
            marker: PhantomData,
        }
    }
}
/// A wrapper to debug the underlying representation of an `IdMap`
pub struct RawDebug<'a, K: IntegerId + 'a, V: 'a, T: 'a + EntryTable<K, V>>(&'a IdMap<K, V, T>);
impl<'a, K, V, T> Debug for RawDebug<'a, K, V, T>
where
    K: IntegerId + Debug,
    V: Debug,
    T: EntryTable<K, V>,
    K: 'a,
    V: 'a,
    T: 'a,
{
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_struct("IdMap")
            .field("entries", self.0.entries.raw_debug())
            .finish()
    }
}
/// Checks if two have the same contents, ignoring the order.
impl<K, V1, T1, V2, T2> PartialEq<IdMap<K, V2, T2>> for IdMap<K, V1, T1>
where
    K: IntegerId,
    V1: PartialEq<V2>,
    T1: EntryTable<K, V1>,
    T2: EntryTable<K, V2>,
{
    fn eq(&self, other: &IdMap<K, V2, T2>) -> bool {
        if self.entries.len() != other.entries.len() {
            return false;
        }
        self.iter().all(|(key, value)| {
            other
                .get(key)
                .map_or(false, |other_value| value == other_value)
        })
    }
}
/// Creates an `IdMap` from a list of key-value pairs
///
/// ## Example
/// ````
/// #[macro_use] extern crate idmap;
/// # fn main() {
/// let map = idmap! {
///     1 => "a",
///     25 => "b"
/// };
/// assert_eq!(map[1], "a");
/// assert_eq!(map[25], "b");
/// assert_eq!(map.get(26), None);
/// // 1 is the first key
/// assert_eq!(map.keys().next(), Some(&1));
/// # }
/// ````
#[macro_export]
macro_rules! idmap {
    ($($key:expr => $value:expr),*) => {
        {
            let entries = vec![$(($key, $value)),*];
            let mut result = $crate::IdMap::with_capacity(entries.len());
            result.extend(entries);
            result
        }
    };
    ($($key:expr => $value:expr,)*) => (idmap!($($key => $value),*));
}

/// Creates an `DirectIdMap` from a list of key-value pairs
///
/// ## Example
/// ````
/// #[macro_use] extern crate idmap;
/// # fn main() {
/// let map = direct_idmap! {
///     1 => "a",
///     25 => "b"
/// };
/// assert_eq!(map[1], "a");
/// assert_eq!(map[25], "b");
/// assert_eq!(map.get(26), None);
/// // 1 is the first key
/// assert_eq!(map.keys().next(), Some(&1));
/// # }
/// ````
#[macro_export]
macro_rules! direct_idmap {
    ($($key:expr => $value:expr),*) => {
        {
            let entries = vec![$(($key, $value)),*];
            let mut result = $crate::IdMap::with_capacity_direct(entries.len());
            result.extend(entries);
            result
        }
    };
    ($($key:expr => $value:expr,)*) => (direct_idmap!($($key => $value),*));
}

/// Creates an `IdSet` from a list of elements
///
/// ## Example
/// ````
/// #[macro_use] extern crate idmap;
/// # fn main() {
/// let set = idset![1, 25];
/// assert_eq!(set[1], true);
/// assert_eq!(set[25], true);
/// assert_eq!(set[26], false);
/// // 1 is the first key
/// assert_eq!(set.iter().next(), Some(1));
/// # }
/// ````
#[macro_export]
macro_rules! idset {
    ($($element:expr),*) => {
        {
            let entries = vec![$($element),*];
            let mut result = $crate::IdSet::with_capacity(entries.len());
            result.extend(entries);
            result
        }
    };
}
impl<K: IntegerId, V, T: EntryTable<K, V>> Default for IdMap<K, V, T> {
    #[inline]
    fn default() -> Self {
        IdMap::new_other()
    }
}
impl<K: IntegerId, V, T: EntryTable<K, V>> Index<K> for IdMap<K, V, T> {
    type Output = V;
    #[inline]
    fn index(&self, key: K) -> &V {
        &self[&key]
    }
}
impl<K: IntegerId, V, T: EntryTable<K, V>> IndexMut<K> for IdMap<K, V, T> {
    #[inline]
    fn index_mut(&mut self, key: K) -> &mut V {
        &mut self[&key]
    }
}
impl<'a, K: IntegerId, V, T: EntryTable<K, V>> Index<&'a K> for IdMap<K, V, T> {
    type Output = V;
    #[inline]
    fn index(&self, key: &'a K) -> &V {
        if let Some(value) = self.get(key) {
            value
        } else {
            panic!("Missing entry for {:?}", key)
        }
    }
}
impl<'a, K: IntegerId, V, T: EntryTable<K, V>> IndexMut<&'a K> for IdMap<K, V, T> {
    #[inline]
    fn index_mut(&mut self, key: &'a K) -> &mut V {
        if let Some(value) = self.get_mut(key) {
            value
        } else {
            panic!("Missing entry for {:?}", key)
        }
    }
}
impl<K: IntegerId, V: Debug, T: EntryTable<K, V>> Debug for IdMap<K, V, T> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let mut d = f.debug_map();
        for (key, value) in self.iter() {
            d.entry(key, value);
        }
        d.finish()
    }
}
/// An entry in an [IdMap]
pub enum Entry<'a, K: IntegerId + 'a, V: 'a, T: 'a + EntryTable<K, V>> {
    /// An entry whose value is present
    Occupied(OccupiedEntry<'a, K, V, T>),
    /// An entry whose value is missing
    Vacant(VacantEntry<'a, K, V, T>),
}
impl<'a, K: IntegerId + 'a, V: 'a, T: EntryTable<K, V>> Entry<'a, K, V, T> {
    /// Return a reference to this entry's value,
    /// initializing it with the specified value if its missing
    #[inline]
    pub fn or_insert(self, value: V) -> &'a mut V {
        self.or_insert_with(|| value)
    }
    /// Return a reference to this entry's value,
    /// using the closure to initialize it if its missing
    #[inline]
    pub fn or_insert_with<F>(self, func: F) -> &'a mut V
    where
        F: FnOnce() -> V,
    {
        match self {
            Entry::Occupied(entry) => entry.value(),
            Entry::Vacant(entry) => entry.or_insert_with(func),
        }
    }
}
/// An entry in an [IdMap] where the value is present
pub struct OccupiedEntry<'a, K: IntegerId + 'a, V: 'a, T: 'a + EntryTable<K, V>> {
    map: &'a mut IdMap<K, V, T>,
    key: K,
}
impl<'a, K: IntegerId + 'a, V: 'a, T: EntryTable<K, V>> OccupiedEntry<'a, K, V, T> {
    /// The key associated with the entry
    #[inline]
    pub fn key(&self) -> &K {
        &self.key
    }
    /// Consume the entry, giving a mutable reference to the value
    /// which is bound to the (mutable) lifetime of the map.
    #[inline]
    pub fn value(self) -> &'a mut V {
        self.map.entries.get_mut(&self.key).unwrap()
    }
    /// Get a reference to the value
    #[inline]
    pub fn get(&self) -> &V {
        self.map.entries.get(&self.key).unwrap()
    }
    /// Borrow a mutable reference to the value
    #[inline]
    pub fn get_mut(&mut self) -> &mut V {
        self.map.entries.get_mut(&self.key).unwrap()
    }
    /// Replace the value in the map, returning its old value
    #[inline]
    pub fn insert(self, value: V) -> V {
        self.map.entries.insert(self.key, value).unwrap()
    }
    /// Remove the value from the map, consuming this entry
    #[inline]
    pub fn remove(self) -> V {
        self.map.entries.swap_remove(&self.key).unwrap()
    }
}
/// An entry in an [IdMap] where the value is *not* present
pub struct VacantEntry<'a, K: IntegerId + 'a, V: 'a, T: EntryTable<K, V> + 'a> {
    map: &'a mut IdMap<K, V, T>,
    key: K,
}
impl<'a, K: IntegerId + 'a, V: 'a, T: EntryTable<K, V> + 'a> VacantEntry<'a, K, V, T> {
    /// Insert a value into this vacant slot,
    /// giving a reference to the new value
    #[inline]
    pub fn insert(self, value: V) -> &'a mut V {
        self.map.entries.insert_vacant(self.key, value)
    }
    /// Call the specified closure to compute a value to insert,
    /// then proceed by calling [VacantEntry::insert]
    #[inline]
    pub fn or_insert_with<F>(self, func: F) -> &'a mut V
    where
        F: FnOnce() -> V,
    {
        self.insert(func())
    }
}
impl<K: IntegerId, V, T: EntryTable<K, V>> IntoIterator for IdMap<K, V, T> {
    type Item = (K, V);
    type IntoIter = <T as IntoIterator>::IntoIter;
    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.entries.into_iter()
    }
}
impl<'a, K: IntegerId + 'a, V: 'a, T: 'a> IntoIterator for &'a IdMap<K, V, T>
where
    T: EntryTable<K, V>,
{
    type Item = (&'a K, &'a V);
    type IntoIter = Iter<'a, K, V, T>;
    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, K, V, T> IntoIterator for &'a mut IdMap<K, V, T>
where
    T: EntryTable<K, V>,
    K: IntegerId + 'a,
    V: 'a,
    T: 'a,
{
    type Item = (&'a K, &'a mut V);
    type IntoIter = IterMut<'a, K, V, T>;
    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}
impl<K: IntegerId, V, T: EntryTable<K, V>> Extend<(K, V)> for IdMap<K, V, T> {
    fn extend<I: IntoIterator<Item = (K, V)>>(&mut self, iter: I) {
        let iter = iter.into_iter();
        if let Some(size) = iter.size_hint().1 {
            self.reserve(size);
        }
        for (key, value) in iter {
            self.insert(key, value);
        }
    }
}
impl<K: IntegerId, V, T: EntryTable<K, V>> FromIterator<(K, V)> for IdMap<K, V, T> {
    #[inline]
    fn from_iter<I>(iterable: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
    {
        let mut result = Self::new_other();
        result.extend(iterable);
        result
    }
}
impl<'a, K, V, T> FromIterator<(&'a K, &'a V)> for IdMap<K, V, T>
where
    K: IntegerId + Clone + 'a,
    V: Clone + 'a,
    T: EntryTable<K, V>,
{
    #[inline]
    fn from_iter<I>(iterable: I) -> Self
    where
        I: IntoIterator<Item = (&'a K, &'a V)>,
    {
        let mut result = Self::new_other();
        result.extend(iterable);
        result
    }
}
impl<'a, K, V, T> Extend<(&'a K, &'a V)> for IdMap<K, V, T>
where
    K: IntegerId + Clone + 'a,
    V: Clone + 'a,
    T: EntryTable<K, V>,
{
    #[inline]
    fn extend<I: IntoIterator<Item = (&'a K, &'a V)>>(&mut self, iter: I) {
        self.extend(
            iter.into_iter()
                .map(|(key, value)| (key.clone(), value.clone())),
        )
    }
}

// TODO: Enable the macro-generated implementations once intellij understands them
/*
macro_rules! delegating_iter {
    ($name:ident, $target:ident, $lifetime:tt, $key:tt, $value:tt, [ $item:ty ], |$handle:ident| $next:expr) => {
        pub struct $name<$lifetime, $key, $value, I>($target<$lifetime, $key, $value, I>)
            where $key: IntegerId + $lifetime, $value: $lifetime, I: 'a + EntryIterable<$key, $value>;
        impl<$lifetime, $key, $value, I> Iterator for $name<$lifetime, $key, $value, I>
            where $key: IntegerId + $lifetime, $value: $lifetime, I: 'a + EntryIterable<$key, $value> {
            type Item = $item;
            #[inline]
            fn size_hint(&self) -> (usize, Option<usize>) {
                self.0.size_hint()
            }
            #[inline]
            fn next(&mut self) -> Option<$item> {
                let $handle = &mut self.0;
                $next
            }
        }
        impl<$lifetime, $key, $value, I> iter::FusedIterator for $name<$lifetime, $key, $value, I>
            where $key: IntegerId + $lifetime, $value: $lifetime,
                  I: 'a + EntryIterable<$key, $value>, I: iter::FusedIterator {}
        impl<$lifetime, $key, $value, I> iter::ExactSizeIterator for $name<$lifetime, $key, $value, I>
            where $key: IntegerId + $lifetime, $value: $lifetime,
                  I: 'a + EntryIterable<$key, $value>, I: iter::ExactSizeIterator {}
        unsafe impl<$lifetime, $key, $value, I> iter::TrustedLen for $name<$lifetime, $key, $value, I>
            where $key: IntegerId + $lifetime, $value: $lifetime,
                  I: 'a + EntryIterable<$key, $value>, I: iter::TrustedLen {}
    };
}

delegating_iter!(Iter, SafeEntries, 'a, K, V, [ (&'a K, &'a V) ], |handle| handle.next().map(|(_, key, value)| (key, value)));
delegating_iter!(IterMut, SafeEntriesMut, 'a, K, V, [ (&'a K, &'a mut V) ], |handle| handle.next().map(|(_, key, value)| (key, value)));
delegating_iter!(Keys, SafeEntries, 'a, K, V, [ &'a K ], |handle| handle.next().map(|(_, key, _)| key));
delegating_iter!(Values, SafeEntries, 'a, K, V, [ &'a V ], |handle| handle.next().map(|(_, _, value)| value));
delegating_iter!(ValuesMut, SafeEntriesMut, 'a, K, V, [ &'a mut V ], |handle| handle.next().map(|(_, _, value)| value));
*/

/*
 * NOTE: These implementations used to be autogenerated by the `delegating_iter` macro,
 * but I've expanded them ahead of time in order to allow intellij's autocomplete to understand it.
 */

/// An iterator over the entries in a map
pub struct Iter<'a, K, V, I>(SafeEntries<'a, K, V, I>)
where
    K: IntegerId + 'a,
    V: 'a,
    I: 'a + EntryIterable<K, V>;
impl<'a, K, V, I> Iterator for Iter<'a, K, V, I>
where
    K: IntegerId + 'a,
    V: 'a,
    I: 'a + EntryIterable<K, V>,
{
    type Item = (&'a K, &'a V);
    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
    #[inline]
    fn next(&mut self) -> Option<(&'a K, &'a V)> {
        self.0.next().map(|(_, key, value)| (key, value))
    }
}
impl<'a, K, V, I> iter::FusedIterator for Iter<'a, K, V, I>
where
    K: IntegerId + 'a,
    V: 'a,
    I: 'a + EntryIterable<K, V>,
    I: iter::FusedIterator,
{
}
impl<'a, K, V, I> iter::ExactSizeIterator for Iter<'a, K, V, I>
where
    K: IntegerId + 'a,
    V: 'a,
    I: 'a + EntryIterable<K, V>,
    I: iter::ExactSizeIterator,
{
}
#[cfg(feature = "nightly")]
unsafe impl<'a, K, V, I> iter::TrustedLen for Iter<'a, K, V, I>
where
    K: IntegerId + 'a,
    V: 'a,
    I: 'a + EntryIterable<K, V>,
    I: iter::TrustedLen,
{
}
impl<'a, K, V, I> Clone for Iter<'a, K, V, I>
where
    K: IntegerId + 'a,
    V: 'a,
    I: 'a + EntryIterable<K, V>,
{
    #[inline]
    fn clone(&self) -> Self {
        Iter(self.0.clone())
    }
}
impl<'a, K, V, I> Debug for Iter<'a, K, V, I>
where
    K: IntegerId + Debug + 'a,
    V: Debug + 'a,
    I: 'a + EntryIterable<K, V>,
{
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_tuple("Iter").field(&self.0).finish()
    }
}

/// An iterator over the keys in a map
pub struct Keys<'a, K, V, I>(SafeEntries<'a, K, V, I>)
where
    K: IntegerId + 'a,
    V: 'a,
    I: 'a + EntryIterable<K, V>;
impl<'a, K, V, I> Iterator for Keys<'a, K, V, I>
where
    K: IntegerId + 'a,
    V: 'a,
    I: 'a + EntryIterable<K, V>,
{
    type Item = &'a K;
    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
    #[inline]
    fn next(&mut self) -> Option<&'a K> {
        self.0.next().map(|(_, key, _)| key)
    }
}
impl<'a, K, V, I> iter::FusedIterator for Keys<'a, K, V, I>
where
    K: IntegerId + 'a,
    V: 'a,
    I: 'a + EntryIterable<K, V>,
    I: iter::FusedIterator,
{
}
impl<'a, K, V, I> iter::ExactSizeIterator for Keys<'a, K, V, I>
where
    K: IntegerId + 'a,
    V: 'a,
    I: 'a + EntryIterable<K, V>,
    I: iter::ExactSizeIterator,
{
}
#[cfg(feature = "nightly")]
unsafe impl<'a, K, V, I> iter::TrustedLen for Keys<'a, K, V, I>
where
    K: IntegerId + 'a,
    V: 'a,
    I: 'a + EntryIterable<K, V>,
    I: iter::TrustedLen,
{
}
impl<'a, K, V, I> Clone for Keys<'a, K, V, I>
where
    K: IntegerId + 'a,
    V: 'a,
    I: 'a + EntryIterable<K, V>,
{
    #[inline]
    fn clone(&self) -> Self {
        Keys(self.0.clone())
    }
}
impl<'a, K, V, I> Debug for Keys<'a, K, V, I>
where
    K: IntegerId + Debug + 'a,
    V: Debug + 'a,
    I: 'a + EntryIterable<K, V>,
{
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_tuple("Keys").field(&self.0).finish()
    }
}

/// An iterator over the values in a map
pub struct Values<'a, K, V, I>(SafeEntries<'a, K, V, I>)
where
    K: IntegerId + 'a,
    V: 'a,
    I: 'a + EntryIterable<K, V>;
impl<'a, K, V, I> Iterator for Values<'a, K, V, I>
where
    K: IntegerId + 'a,
    V: 'a,
    I: 'a + EntryIterable<K, V>,
{
    type Item = &'a V;
    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
    #[inline]
    fn next(&mut self) -> Option<&'a V> {
        self.0.next().map(|(_, _, value)| value)
    }
}
impl<'a, K, V, I> iter::FusedIterator for Values<'a, K, V, I>
where
    K: IntegerId + 'a,
    V: 'a,
    I: 'a + EntryIterable<K, V>,
    I: iter::FusedIterator,
{
}
impl<'a, K, V, I> iter::ExactSizeIterator for Values<'a, K, V, I>
where
    K: IntegerId + 'a,
    V: 'a,
    I: 'a + EntryIterable<K, V>,
    I: iter::ExactSizeIterator,
{
}
#[cfg(feature = "nightly")]
unsafe impl<'a, K, V, I> iter::TrustedLen for Values<'a, K, V, I>
where
    K: IntegerId + 'a,
    V: 'a,
    I: 'a + EntryIterable<K, V>,
    I: iter::TrustedLen,
{
}
impl<'a, K, V, I> Clone for Values<'a, K, V, I>
where
    K: IntegerId + 'a,
    V: 'a,
    I: 'a + EntryIterable<K, V>,
{
    #[inline]
    fn clone(&self) -> Self {
        Values(self.0.clone())
    }
}
impl<'a, K, V, I> Debug for Values<'a, K, V, I>
where
    K: IntegerId + Debug + 'a,
    V: Debug + 'a,
    I: 'a + EntryIterable<K, V>,
{
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_tuple("Values").field(&self.0).finish()
    }
}

/// An iterator over mutable references to the values in a map
pub struct ValuesMut<'a, K, V, I>(SafeEntriesMut<'a, K, V, I>)
where
    K: IntegerId + 'a,
    V: 'a,
    I: 'a + EntryIterable<K, V>;
impl<'a, K, V, I> Iterator for ValuesMut<'a, K, V, I>
where
    K: IntegerId + 'a,
    V: 'a,
    I: 'a + EntryIterable<K, V>,
{
    type Item = &'a mut V;
    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
    #[inline]
    fn next(&mut self) -> Option<&'a mut V> {
        self.0.next().map(|(_, _, value)| value)
    }
}
impl<'a, K, V, I> iter::FusedIterator for ValuesMut<'a, K, V, I>
where
    K: IntegerId + 'a,
    V: 'a,
    I: 'a + EntryIterable<K, V>,
    I: iter::FusedIterator,
{
}
impl<'a, K, V, I> iter::ExactSizeIterator for ValuesMut<'a, K, V, I>
where
    K: IntegerId + 'a,
    V: 'a,
    I: 'a + EntryIterable<K, V>,
    I: iter::ExactSizeIterator,
{
}
#[cfg(feature = "nightly")]
unsafe impl<'a, K, V, I> iter::TrustedLen for ValuesMut<'a, K, V, I>
where
    K: IntegerId + 'a,
    V: 'a,
    I: 'a + EntryIterable<K, V>,
    I: iter::TrustedLen,
{
}

/// An iterator over the entries in a map, giving mutable references to the values
pub struct IterMut<'a, K, V, I>(SafeEntriesMut<'a, K, V, I>)
where
    K: IntegerId + 'a,
    V: 'a,
    I: 'a + EntryIterable<K, V>;
impl<'a, K, V, I> Iterator for IterMut<'a, K, V, I>
where
    K: IntegerId + 'a,
    V: 'a,
    I: 'a + EntryIterable<K, V>,
{
    type Item = (&'a K, &'a mut V);
    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
    #[inline]
    fn next(&mut self) -> Option<(&'a K, &'a mut V)> {
        self.0.next().map(|(_, key, value)| (key, value))
    }
}
impl<'a, K, V, I> iter::FusedIterator for IterMut<'a, K, V, I>
where
    K: IntegerId + 'a,
    V: 'a,
    I: 'a + EntryIterable<K, V>,
    I: iter::FusedIterator,
{
}
impl<'a, K, V, I> iter::ExactSizeIterator for IterMut<'a, K, V, I>
where
    K: IntegerId + 'a,
    V: 'a,
    I: 'a + EntryIterable<K, V>,
    I: iter::ExactSizeIterator,
{
}
#[cfg(feature = "nightly")]
unsafe impl<'a, K, V, I> iter::TrustedLen for IterMut<'a, K, V, I>
where
    K: IntegerId + 'a,
    V: 'a,
    I: 'a + EntryIterable<K, V>,
    I: iter::TrustedLen,
{
}

/// Support function that panics if an id is invalid
#[doc(hidden)]
#[cold]
#[inline(never)]
pub fn _invalid_id(id: u64) -> ! {
    panic!("ID is invalid: {}", id);
}
