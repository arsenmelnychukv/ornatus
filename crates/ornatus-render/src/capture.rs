//! Rendering the model to an image, for screenshot export (decision 0005).
//!
//! The model is drawn into an offscreen texture with a transparent
//! background, copied to a buffer, and read back without blocking, since the
//! browser cannot wait for the GPU.

use std::sync::mpsc;

/// Rows copied from a texture to a buffer must start at multiples of this.
const ROW_ALIGNMENT: u32 = 256;

/// A finished screenshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedImage {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// 8-bit RGBA, straight (not premultiplied) alpha, top row first.
    pub rgba: Vec<u8>,
}

/// A screenshot on its way back from the GPU.
pub struct ImageCapture {
    pub(crate) buffer: wgpu::Buffer,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) padded_row: u32,
    pub(crate) bgra: bool,
    pub(crate) receiver: mpsc::Receiver<Result<(), wgpu::BufferAsyncError>>,
}

/// Bytes per row of the copy buffer for an image `width` pixels wide.
pub(crate) fn padded_row_bytes(width: u32) -> u32 {
    (width * 4).div_ceil(ROW_ALIGNMENT) * ROW_ALIGNMENT
}

/// Remove row padding, put the channels in RGBA order and undo the alpha
/// multiplication that blending applied to the color.
pub(crate) fn finish_pixels(
    padded: &[u8],
    width: u32,
    height: u32,
    padded_row: u32,
    bgra: bool,
) -> Vec<u8> {
    let (width, height, padded_row) = (width as usize, height as usize, padded_row as usize);
    let mut out = Vec::with_capacity(width * height * 4);
    for row in 0..height {
        let start = row * padded_row;
        for pixel in padded[start..start + width * 4].as_chunks::<4>().0 {
            let [mut r, g, mut b, a] = *pixel;
            if bgra {
                std::mem::swap(&mut r, &mut b);
            }
            let straight = |c: u8| {
                if a == 0 || a == 255 {
                    if a == 0 { 0 } else { c }
                } else {
                    ((u32::from(c) * 255 + u32::from(a) / 2) / u32::from(a)).min(255) as u8
                }
            };
            out.extend([straight(r), straight(g), straight(b), a]);
        }
    }
    out
}

impl ImageCapture {
    /// Check whether the GPU is done. Returns `None` while it is still
    /// working, so call it again on a later frame.
    pub fn poll(&self, device: &wgpu::Device) -> Option<Result<CapturedImage, String>> {
        let _ = device.poll(wgpu::PollType::Poll);
        match self.receiver.try_recv() {
            Ok(Ok(())) => {
                let image = match self.buffer.slice(..).get_mapped_range() {
                    Ok(view) => Ok(CapturedImage {
                        width: self.width,
                        height: self.height,
                        rgba: finish_pixels(
                            &view,
                            self.width,
                            self.height,
                            self.padded_row,
                            self.bgra,
                        ),
                    }),
                    Err(error) => Err(format!("could not read the image: {error}")),
                };
                self.buffer.unmap();
                Some(image)
            }
            Ok(Err(error)) => Some(Err(format!("the GPU could not return the image: {error}"))),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Some(Err("the capture was dropped".to_owned()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_are_padded_to_the_copy_alignment() {
        assert_eq!(padded_row_bytes(1), 256);
        assert_eq!(padded_row_bytes(64), 256);
        assert_eq!(padded_row_bytes(65), 512);
    }

    #[test]
    fn padding_is_removed_and_channels_reordered() {
        // Two pixels per row, rows padded to 256 bytes, BGRA order.
        let mut padded = vec![0u8; 256 * 2];
        padded[..8].copy_from_slice(&[10, 20, 30, 255, 1, 2, 3, 255]);
        padded[256..264].copy_from_slice(&[40, 50, 60, 255, 7, 8, 9, 255]);
        let out = finish_pixels(&padded, 2, 2, 256, true);
        assert_eq!(
            out,
            [30, 20, 10, 255, 3, 2, 1, 255, 60, 50, 40, 255, 9, 8, 7, 255]
        );
        let rgba = finish_pixels(&padded, 2, 2, 256, false);
        assert_eq!(&rgba[..4], [10, 20, 30, 255]);
    }

    #[test]
    fn blended_color_is_divided_back_by_alpha() {
        let mut padded = vec![0u8; 256];
        // Half transparent red whose color was multiplied by alpha, and a clear pixel.
        padded[..8].copy_from_slice(&[128, 0, 0, 128, 0, 0, 0, 0]);
        let out = finish_pixels(&padded, 2, 1, 256, false);
        assert_eq!(&out[..4], [255, 0, 0, 128]);
        assert_eq!(&out[4..8], [0, 0, 0, 0]);
    }
}
