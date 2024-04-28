use std::cell::RefMut;

use futures_util::AsyncWriteExt;
use gtk::{gio, glib, subclass::prelude::*};
use libp2p::Stream;

mod imp {
    use std::cell::RefCell;

    use super::*;

    #[derive(Default)]
    pub struct OutputStream {
        pub(super) inner: RefCell<Option<Stream>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for OutputStream {
        const NAME: &'static str = "DeltaOutputStream";
        type Type = super::OutputStream;
        type ParentType = gio::OutputStream;
    }

    impl ObjectImpl for OutputStream {}

    impl OutputStreamImpl for OutputStream {
        fn write(
            &self,
            buffer: &[u8],
            _cancellable: Option<&gio::Cancellable>,
        ) -> Result<usize, glib::Error> {
            async_std::task::block_on(self.obj().inner().write(buffer))
                .map_err(|e| glib::Error::new(gio::IOErrorEnum::Failed, &e.to_string()))
        }

        fn close(&self, _cancellable: Option<&gio::Cancellable>) -> Result<(), glib::Error> {
            tracing::debug!("Close called");

            async_std::task::block_on(self.obj().inner().close())
                .map_err(|e| glib::Error::new(gio::IOErrorEnum::Failed, &e.to_string()))
        }

        fn flush(&self, _cancellable: Option<&gio::Cancellable>) -> Result<(), glib::Error> {
            async_std::task::block_on(self.obj().inner().flush())
                .map_err(|e| glib::Error::new(gio::IOErrorEnum::Failed, &e.to_string()))
        }

        fn splice(
            &self,
            input_stream: &gio::InputStream,
            flags: gio::OutputStreamSpliceFlags,
            cancellable: Option<&gio::Cancellable>,
        ) -> Result<usize, glib::Error> {
            tracing::warn!("Splice called");

            self.parent_splice(input_stream, flags, cancellable)
        }
    }
}

glib::wrapper! {
    pub struct OutputStream(ObjectSubclass<imp::OutputStream>)
        @extends gio::OutputStream;
}

impl OutputStream {
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
