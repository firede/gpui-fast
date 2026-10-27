//! Images on the Wayland clipboard.
//!
//! Upstream offers what the app copies under the text MIME types only, and
//! sends the item's text whatever type is asked for, so an image the app
//! copies never reaches another program. Reading, it takes an offer's text,
//! and its image only when it has no text, so an image copied together with
//! text (a browser's "Copy Image" offers both) comes in as text alone.
//!
//! Here each image of a copied item is offered under its format's MIME type
//! as well, and sent when that type is asked for. Reading takes both the
//! offer's text and its image.

use std::os::fd::OwnedFd;

use gpui::{ClipboardEntry, ClipboardItem};
use wayland_client::{Connection, protocol::wl_data_source::WlDataSource};

use crate::linux::wayland::clipboard::{Clipboard, DataOffer, ReceiveData};

/// Offers the images of what the app copied, after upstream has offered its
/// text.
pub(crate) fn offer_images(clipboard: &Clipboard, data_source: &WlDataSource) {
    if let Some(item) = clipboard.contents.as_ref() {
        for mime_type in image_mime_types(item) {
            data_source.offer(mime_type.to_string());
        }
    }
}

/// Sends `contents` to a program pasting it as `mime_type`.
pub(crate) fn send(
    clipboard: &Clipboard,
    contents: Option<&ClipboardItem>,
    mime_type: &str,
    fd: OwnedFd,
) {
    if let Some(bytes) = contents.and_then(|contents| bytes_for(contents, mime_type)) {
        clipboard.send_bytes(fd, bytes);
    }
}

/// Reads another program's offer: its text and its image, when it has both.
pub(crate) fn read<T: ReceiveData>(
    offer: &DataOffer<T>,
    connection: &Connection,
) -> Option<ClipboardItem> {
    let mut entries = Vec::new();
    if let Some(text) = offer.read_text(connection) {
        entries.extend(text.into_entries());
    }
    if let Some(image) = offer.read_image(connection) {
        entries.extend(image.into_entries());
    }
    (!entries.is_empty()).then_some(ClipboardItem { entries })
}

fn image_mime_types(item: &ClipboardItem) -> impl Iterator<Item = &'static str> + '_ {
    item.entries().iter().filter_map(|entry| match entry {
        ClipboardEntry::Image(image) => Some(image.format.mime_type()),
        _ => None,
    })
}

/// The image in `mime_type`'s format, or else the text, which upstream sends
/// for every type.
fn bytes_for(item: &ClipboardItem, mime_type: &str) -> Option<Vec<u8>> {
    let image = item.entries().iter().find_map(|entry| match entry {
        ClipboardEntry::Image(image) if image.format.mime_type() == mime_type => {
            Some(image.bytes.clone())
        }
        _ => None,
    });
    image.or_else(|| item.text().map(String::into_bytes))
}

#[cfg(test)]
mod tests {
    use gpui::{ClipboardEntry, ClipboardItem, ClipboardString, Image, ImageFormat};

    use super::{bytes_for, image_mime_types};

    fn item() -> ClipboardItem {
        ClipboardItem {
            entries: vec![
                ClipboardEntry::String(ClipboardString::new("caption".into())),
                ClipboardEntry::Image(Image::from_bytes(ImageFormat::Png, vec![1, 2, 3])),
            ],
        }
    }

    #[test]
    fn images_are_offered_under_their_formats() {
        assert_eq!(image_mime_types(&item()).collect::<Vec<_>>(), ["image/png"]);
        let text = ClipboardItem::new_string("caption".into());
        assert_eq!(image_mime_types(&text).count(), 0);
    }

    #[test]
    fn an_image_type_gets_the_image_and_other_types_the_text() {
        assert_eq!(bytes_for(&item(), "image/png"), Some(vec![1, 2, 3]));
        assert_eq!(
            bytes_for(&item(), "text/plain;charset=utf-8"),
            Some(b"caption".to_vec())
        );
        assert_eq!(bytes_for(&item(), "image/jpeg"), Some(b"caption".to_vec()));
    }
}
