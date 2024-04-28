use std::cell::RefMut;

use futures_util::{AsyncReadExt, AsyncWriteExt};
use gtk::{gio, glib, subclass::prelude::*};
use libp2p::Stream;

mod imp {
    use std::cell::RefCell;

    use super::*;

    #[derive(Default)]
    pub struct InputStream {
        pub(super) inner: RefCell<Option<Stream>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for InputStream {
        const NAME: &'static str = "DeltaInputStream";
        type Type = super::InputStream;
        type ParentType = gio::InputStream;
    }

    impl ObjectImpl for InputStream {}

    impl InputStreamImpl for InputStream {
        fn read(
            &self,
            buffer: &mut [u8],
            _cancellable: Option<&gio::Cancellable>,
        ) -> Result<usize, glib::Error> {
            async_std::task::block_on(self.obj().inner().read(buffer))
                .map_err(|e| glib::Error::new(gio::IOErrorEnum::Failed, &e.to_string()))
        }

        fn close(&self, _cancellable: Option<&gio::Cancellable>) -> Result<(), glib::Error> {
            tracing::debug!("Close called");

            async_std::task::block_on(self.obj().inner().close())
                .map_err(|e| glib::Error::new(gio::IOErrorEnum::Failed, &e.to_string()))
        }

        fn skip(
            &self,
            count: usize,
            cancellable: Option<&gio::Cancellable>,
        ) -> Result<usize, glib::Error> {
            tracing::warn!("Skip called");

            self.parent_skip(count, cancellable)
        }
    }
}

glib::wrapper! {
    pub struct InputStream(ObjectSubclass<imp::InputStream>)
        @extends gio::InputStream;
}

impl InputStream {
    pub fn new(stream: Stream) -> Self {
        let this = glib::Object::new::<Self>();
        this.imp().inner.replace(Some(stream));
        this
    }

    fn inner(&self) -> RefMut<'_, Stream> {
        RefMut::map(self.imp().inner.borrow_mut(), |inner| {
            inner.as_mut().unwrap()
        })
    }
}
