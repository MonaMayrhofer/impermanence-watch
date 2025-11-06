use std::{collections::HashMap, hash::Hash};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum LeftRightBoth<T> {
    Left(T),
    Right(T),
    Both(T, T),
}

impl<T> LeftRightBoth<T> {
    pub(crate) fn with_right(self, right: T) -> Self {
        match self {
            LeftRightBoth::Left(l) => LeftRightBoth::Both(l, right),
            LeftRightBoth::Right(_) => LeftRightBoth::Right(right),
            LeftRightBoth::Both(left, _) => LeftRightBoth::Both(left, right),
        }
    }
}

pub(crate) fn hashmap_diff<TKey, TVal>(
    a: HashMap<TKey, TVal>,
    b: HashMap<TKey, TVal>,
) -> HashMap<TKey, LeftRightBoth<TVal>>
where
    TKey: Eq + Hash,
{
    let mut result = HashMap::new();

    for (key, value) in a {
        result.insert(key, LeftRightBoth::Left(value));
    }

    for (key, value) in b {
        if let Some(old) = result.remove(&key) {
            result.insert(key, old.with_right(value));
        } else {
            result.insert(key, LeftRightBoth::Right(value));
        }
    }

    result
}
