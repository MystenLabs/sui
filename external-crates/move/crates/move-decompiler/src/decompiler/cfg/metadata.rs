// Copyright (c) Verichains, 2023

use std::{
    any::{Any, TypeId},
    cell::{RefCell, RefMut, Ref},
    collections::HashMap,
    fmt::Debug,
    ops::{Deref, DerefMut},
    rc::Rc,
};

#[derive(Clone, Debug)]
pub struct Metadata {
    metadata: Rc<RefCell<HashMap<TypeId, Box<dyn Any>>>>,
}

impl Default for Metadata {
    fn default() -> Self {
        Self::new()
    }
}

pub struct MetadataRef<'a> {
    _borrow: Ref<'a, HashMap<TypeId, Box<dyn Any>>>,
}

impl<'a> MetadataRef<'a> {
    pub fn get<T: 'static>(&self) -> Option<&T> {
        let key = TypeId::of::<T>();
        self._borrow
            .get(&key)
            .and_then(|boxed| boxed.downcast_ref::<T>())
    }

    pub fn get_or_default<T: 'static + Default + Clone>(&self) -> T {
        let key = TypeId::of::<T>();
        self._borrow
            .get(&key)
            .map(|boxed| boxed.downcast_ref::<T>().unwrap().clone())
            .unwrap_or_default()
    }
}

pub struct MetadataRefMut<'a> {
    _borrow: RefMut<'a, HashMap<TypeId, Box<dyn Any>>>,
}

impl<'a> MetadataRefMut<'a> {
    pub fn get<T: 'static>(&self) -> Option<&T> {
        let key = TypeId::of::<T>();
        self._borrow
            .get(&key)
            .and_then(|boxed| boxed.downcast_ref::<T>())
    }

    pub fn get_mut<T: 'static>(&mut self) -> Option<&mut T> {
        let key = TypeId::of::<T>();
        self._borrow
            .get_mut(&key)
            .and_then(|boxed| boxed.downcast_mut::<T>())
    }

    pub fn set<T: 'static>(&mut self, value: T) {
        let key = TypeId::of::<T>();
        self._borrow.insert(key, Box::new(value));
    }
    pub fn get_or_default<T: 'static + Default>(&mut self) -> &mut T {
        let key = TypeId::of::<T>();
        self._borrow
            .entry(key)
            .or_insert_with(|| Box::new(T::default()))
            .downcast_mut::<T>()
            .unwrap()
    }
}

impl Metadata {
    pub fn new() -> Self {
        Self {
            metadata: Rc::new(RefCell::new(HashMap::new())),
        }
    }

    pub fn borrow_holder<'a>(&'a self) -> MetadataRef<'a> {
        MetadataRef {
            _borrow: self.metadata.borrow(),
        }

    }

    pub fn borrow_holder_mut<'a>(&'a self) -> MetadataRefMut<'a> {
        MetadataRefMut {
            _borrow: self.metadata.borrow_mut(),
        }
    }
    // pub fn for_type<T>(&self) -> MetadataRefMut<T> {
    //     let key = TypeId::of::<T>();

    //     MetadataRefMut {
    //         _borrow: self.metadata.borrow_mut(),
    //         key,
    //         _phantom: PhantomData,
    //     }
    // }
}

#[derive(Clone, Debug)]
pub struct WithMetadata<Inner: Clone> {
    inner: Inner,
    metadata: Metadata,
}

impl<Inner: Clone> WithMetadata<Inner> {
    pub fn new(inner: Inner) -> Self {
        Self {
            inner,
            metadata: Metadata::new(),
        }
    }

    pub fn meta(&self) -> MetadataRef {
        self.metadata.borrow_holder()
    }

    pub fn meta_mut(&mut self) -> MetadataRefMut {
        self.metadata.borrow_holder_mut()
    }

    pub fn inner(&self) -> &Inner {
        &self.inner
    }

    pub fn inner_mut(&mut self) -> &mut Inner {
        &mut self.inner
    }
}

impl<Inner: Clone> Deref for WithMetadata<Inner> {
    fn deref(&self) -> &Self::Target {
        &self.inner
    }

    type Target = Inner;
}

impl<Inner: Clone> DerefMut for WithMetadata<Inner> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

pub trait WithMetadataExt: Sized {
    fn with_metadata(self) -> WithMetadata<Self>
    where
        Self: Clone;
}

impl<Inner: Clone> WithMetadataExt for Inner {
    fn with_metadata(self) -> WithMetadata<Self> {
        WithMetadata::new(self)
    }
}
