pub struct Thunk<T> {
    data: ThunkData<T>,
}

pub enum ThunkData<T> {
    Present(T),
    Pending(Option<Box<dyn FnOnce() -> T>>),
}
impl<T> Thunk<T> {
    pub fn get(&mut self) -> &T {
        if let ThunkData::Present(ref p) = self.data {
            p
        } else {
            self.data = {
                let ThunkData::Pending(func) = &mut self.data else {
                    unreachable!()
                };
                ThunkData::Present(func.take().unwrap()())
            };
            if let ThunkData::Present(ref p) = self.data {
                p
            } else {
                unreachable!()
            }
        }
    }

    pub fn get_mut(&mut self) -> &mut T {
        if let ThunkData::Present(ref mut p) = self.data {
            p
        } else {
            self.data = {
                let ThunkData::Pending(func) = &mut self.data else {
                    unreachable!()
                };
                ThunkData::Present(func.take().unwrap()())
            };
            if let ThunkData::Present(ref mut p) = self.data {
                p
            } else {
                unreachable!()
            }
        }
    }

    pub fn present(it: T) -> Thunk<T> {
        Thunk {
            data: ThunkData::Present(it),
        }
    }
    pub fn lazy(it: impl FnOnce() -> T + 'static) -> Thunk<T> {
        Thunk {
            data: ThunkData::Pending(Some(Box::new(it))),
        }
    }
}
