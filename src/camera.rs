// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Michael Schaefer <https://github.com/mischa-robots/agentic-robot>

//! Stereo camera capture with trait abstraction for testability.
//!
//! The [`CameraCapture`] trait defines the interface for capturing stereo frames.
//! [`GStreamerCapture`] implements it using OpenCV with GStreamer CSI pipelines.
//! [`MockCapture`] provides a test double.

use std::path::{Path, PathBuf};
#[cfg(feature = "camera")]
use std::sync::Mutex;

use crate::error::AppError;

/// A captured stereo frame (left + right stacked horizontally).
#[derive(Debug, Clone)]
pub struct StereoFrame {
    /// JPEG-encoded image data (left | right, side-by-side).
    pub jpeg_data: Vec<u8>,
    /// Width of the combined image.
    pub width: u32,
    /// Height of the combined image.
    pub height: u32,
}

/// Trait for camera capture implementations.
pub trait CameraCapture: Send + Sync {
    /// Capture a stereo frame from both cameras.
    ///
    /// Returns the frame as JPEG data with both camera views stacked horizontally.
    fn capture(&mut self) -> Result<StereoFrame, AppError>;

    /// Release camera resources.
    fn release(&mut self);
}

/// GStreamer-based CSI camera capture for Jetson Nano.
///
/// Uses `nvarguscamerasrc` to access both CSI camera sensors and stacks
/// the frames horizontally into a single image.
#[allow(dead_code)] // Fields used in camera feature-gated impl
pub struct GStreamerCapture {
    left_pipeline: String,
    right_pipeline: String,
    width: u32,
    height: u32,
    // OpenCV VideoCapture handles wrapped in Mutex for Sync safety.
    // VideoCapture contains *mut c_void which isn't Sync, but is only
    // accessed via &mut self so the Mutex is uncontended.
    #[cfg(feature = "camera")]
    left_cap: Mutex<Option<opencv::videoio::VideoCapture>>,
    #[cfg(feature = "camera")]
    right_cap: Mutex<Option<opencv::videoio::VideoCapture>>,
}

impl GStreamerCapture {
    /// Create a new capture instance for the given resolution.
    ///
    /// Cameras are initialized lazily on first capture.
    pub fn new(width: u32, height: u32, swap_cameras: bool) -> Self {
        let (left_sensor_id, right_sensor_id) = sensor_ids(swap_cameras);
        let left_pipeline = gstreamer_pipeline(left_sensor_id, width, height);
        let right_pipeline = gstreamer_pipeline(right_sensor_id, width, height);

        Self {
            left_pipeline,
            right_pipeline,
            width,
            height,
            #[cfg(feature = "camera")]
            left_cap: Mutex::new(None),
            #[cfg(feature = "camera")]
            right_cap: Mutex::new(None),
        }
    }
}

fn sensor_ids(swap_cameras: bool) -> (u32, u32) {
    if swap_cameras { (1, 0) } else { (0, 1) }
}

/// Build a GStreamer pipeline string for nvarguscamerasrc.
fn gstreamer_pipeline(sensor_id: u32, width: u32, height: u32) -> String {
    format!(
        "nvarguscamerasrc sensor-id={sensor_id} ! \
         video/x-raw(memory:NVMM),width={width},height={height},framerate=30/1 ! \
         nvvidconv ! video/x-raw,format=BGRx ! \
         videoconvert ! video/x-raw,format=BGR ! \
         queue max-size-buffers=1 leaky=downstream ! \
         appsink max-buffers=1 drop=true sync=false"
    )
}

#[cfg(feature = "camera")]
impl CameraCapture for GStreamerCapture {
    fn capture(&mut self) -> Result<StereoFrame, AppError> {
        use opencv::core::{Mat, Vector};
        use opencv::imgcodecs;
        use opencv::prelude::*;
        use opencv::videoio::{self, VideoCaptureTrait, VideoCaptureTraitConst};

        let mut left_guard = self.left_cap.lock().unwrap();
        let mut right_guard = self.right_cap.lock().unwrap();

        // Initialize cameras lazily
        if left_guard.is_none() {
            let left = videoio::VideoCapture::from_file(&self.left_pipeline, videoio::CAP_GSTREAMER)
                .map_err(|e| AppError::Camera(format!("left camera init failed: {e}")))?;
            if !left.is_opened().unwrap_or(false) {
                return Err(AppError::Camera("left camera not opened".to_string()));
            }
            *left_guard = Some(left);
        }

        if right_guard.is_none() {
            let right =
                videoio::VideoCapture::from_file(&self.right_pipeline, videoio::CAP_GSTREAMER)
                    .map_err(|e| AppError::Camera(format!("right camera init failed: {e}")))?;
            if !right.is_opened().unwrap_or(false) {
                return Err(AppError::Camera("right camera not opened".to_string()));
            }
            *right_guard = Some(right);
        }

        let left_cap = left_guard.as_mut().unwrap();
        let right_cap = right_guard.as_mut().unwrap();

        // Capture frames
        let mut left_frame = Mat::default();
        let mut right_frame = Mat::default();

        left_cap
            .read(&mut left_frame)
            .map_err(|e| AppError::Camera(format!("left frame read failed: {e}")))?;
        right_cap
            .read(&mut right_frame)
            .map_err(|e| AppError::Camera(format!("right frame read failed: {e}")))?;

        if left_frame.empty() || right_frame.empty() {
            return Err(AppError::Camera("empty frame captured".to_string()));
        }

        // Stack horizontally (hconcat)
        let mut combined = Mat::default();
        let frames = Vector::<Mat>::from_iter([left_frame, right_frame]);
        opencv::core::hconcat(&frames, &mut combined)
            .map_err(|e| AppError::Camera(format!("frame concat failed: {e}")))?;

        // Encode as JPEG
        let mut buf = Vector::<u8>::new();
        let params = Vector::<i32>::from_iter([imgcodecs::IMWRITE_JPEG_QUALITY, 85]);
        imgcodecs::imencode(".jpg", &combined, &mut buf, &params)
            .map_err(|e| AppError::Camera(format!("JPEG encode failed: {e}")))?;

        Ok(StereoFrame {
            jpeg_data: buf.to_vec(),
            width: self.width * 2,
            height: self.height,
        })
    }

    fn release(&mut self) {
        *self.left_cap.lock().unwrap() = None;
        *self.right_cap.lock().unwrap() = None;
    }
}

#[cfg(not(feature = "camera"))]
impl CameraCapture for GStreamerCapture {
    fn capture(&mut self) -> Result<StereoFrame, AppError> {
        Err(AppError::Camera(
            "camera capture requires the `camera` feature flag".to_string(),
        ))
    }

    fn release(&mut self) {}
}

/// Save a stereo frame to disk as JPEG.
pub fn save_frame(frame: &StereoFrame, path: &Path) -> Result<PathBuf, AppError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, &frame.jpeg_data)?;
    Ok(path.to_path_buf())
}

// --- Mock for testing ---

/// Mock camera that returns a pre-configured test frame.
#[cfg(test)]
pub struct MockCapture {
    frame: StereoFrame,
    capture_count: u32,
}

#[cfg(test)]
impl MockCapture {
    pub fn new() -> Self {
        // Create a minimal valid JPEG (1x1 pixel)
        Self {
            frame: StereoFrame {
                jpeg_data: minimal_jpeg(),
                width: 1280,
                height: 480,
            },
            capture_count: 0,
        }
    }

    pub fn with_frame(frame: StereoFrame) -> Self {
        Self {
            frame,
            capture_count: 0,
        }
    }

    pub fn capture_count(&self) -> u32 {
        self.capture_count
    }
}

#[cfg(test)]
impl CameraCapture for MockCapture {
    fn capture(&mut self) -> Result<StereoFrame, AppError> {
        self.capture_count += 1;
        Ok(self.frame.clone())
    }

    fn release(&mut self) {}
}

/// Generate a minimal valid JPEG for testing.
#[cfg(test)]
fn minimal_jpeg() -> Vec<u8> {
    // Minimal 1x1 white JPEG
    vec![
        0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46, 0x00, 0x01, 0x01, 0x00,
        0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0xFF, 0xDB, 0x00, 0x43, 0x00, 0x08, 0x06, 0x06,
        0x07, 0x06, 0x05, 0x08, 0x07, 0x07, 0x07, 0x09, 0x09, 0x08, 0x0A, 0x0C, 0x14, 0x0D,
        0x0C, 0x0B, 0x0B, 0x0C, 0x19, 0x12, 0x13, 0x0F, 0x14, 0x1D, 0x1A, 0x1F, 0x1E, 0x1D,
        0x1A, 0x1C, 0x1C, 0x20, 0x24, 0x2E, 0x27, 0x20, 0x22, 0x2C, 0x23, 0x1C, 0x1C, 0x28,
        0x37, 0x29, 0x2C, 0x30, 0x31, 0x34, 0x34, 0x34, 0x1F, 0x27, 0x39, 0x3D, 0x38, 0x32,
        0x3C, 0x2E, 0x33, 0x34, 0x32, 0xFF, 0xC0, 0x00, 0x0B, 0x08, 0x00, 0x01, 0x00, 0x01,
        0x01, 0x01, 0x11, 0x00, 0xFF, 0xC4, 0x00, 0x1F, 0x00, 0x00, 0x01, 0x05, 0x01, 0x01,
        0x01, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x02,
        0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0xFF, 0xC4, 0x00, 0xB5, 0x10,
        0x00, 0x02, 0x01, 0x03, 0x03, 0x02, 0x04, 0x03, 0x05, 0x05, 0x04, 0x04, 0x00, 0x00,
        0x01, 0x7D, 0x01, 0x02, 0x03, 0x00, 0x04, 0x11, 0x05, 0x12, 0x21, 0x31, 0x41, 0x06,
        0x13, 0x51, 0x61, 0x07, 0x22, 0x71, 0x14, 0x32, 0x81, 0x91, 0xA1, 0x08, 0x23, 0x42,
        0xB1, 0xC1, 0x15, 0x52, 0xD1, 0xF0, 0x24, 0x33, 0x62, 0x72, 0x82, 0x09, 0x0A, 0x16,
        0x17, 0x18, 0x19, 0x1A, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2A, 0x34, 0x35, 0x36, 0x37,
        0x38, 0x39, 0x3A, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4A, 0x53, 0x54, 0x55,
        0x56, 0x57, 0x58, 0x59, 0x5A, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69, 0x6A, 0x73,
        0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7A, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89,
        0x8A, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9A, 0xA2, 0xA3, 0xA4, 0xA5,
        0xA6, 0xA7, 0xA8, 0xA9, 0xAA, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6, 0xB7, 0xB8, 0xB9, 0xBA,
        0xC2, 0xC3, 0xC4, 0xC5, 0xC6, 0xC7, 0xC8, 0xC9, 0xCA, 0xD2, 0xD3, 0xD4, 0xD5, 0xD6,
        0xD7, 0xD8, 0xD9, 0xDA, 0xE1, 0xE2, 0xE3, 0xE4, 0xE5, 0xE6, 0xE7, 0xE8, 0xE9, 0xEA,
        0xF1, 0xF2, 0xF3, 0xF4, 0xF5, 0xF6, 0xF7, 0xF8, 0xF9, 0xFA, 0xFF, 0xDA, 0x00, 0x08,
        0x01, 0x01, 0x00, 0x00, 0x3F, 0x00, 0x7B, 0x94, 0x11, 0x00, 0x00, 0x00, 0x00, 0x00,
        0xFF, 0xD9,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_capture_returns_frame() {
        let mut cam = MockCapture::new();
        let frame = cam.capture().unwrap();
        assert_eq!(frame.width, 1280);
        assert_eq!(frame.height, 480);
        assert!(!frame.jpeg_data.is_empty());
        assert_eq!(cam.capture_count(), 1);
    }

    #[test]
    fn mock_capture_increments_count() {
        let mut cam = MockCapture::new();
        cam.capture().unwrap();
        cam.capture().unwrap();
        cam.capture().unwrap();
        assert_eq!(cam.capture_count(), 3);
    }

    #[test]
    fn save_frame_creates_file() {
        let frame = StereoFrame {
            jpeg_data: vec![0xFF, 0xD8, 0xFF, 0xD9], // minimal JPEG markers
            width: 1280,
            height: 480,
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test_frame.jpg");

        let result = save_frame(&frame, &path).unwrap();
        assert_eq!(result, path);
        assert!(path.exists());
        assert_eq!(std::fs::read(&path).unwrap(), frame.jpeg_data);
    }

    #[test]
    fn save_frame_creates_parent_dirs() {
        let frame = StereoFrame {
            jpeg_data: vec![0xFF, 0xD8, 0xFF, 0xD9],
            width: 1280,
            height: 480,
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("dir").join("frame.jpg");

        save_frame(&frame, &path).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn gstreamer_pipeline_format() {
        let pipeline = gstreamer_pipeline(0, 640, 480);
        assert!(pipeline.contains("sensor-id=0"));
        assert!(pipeline.contains("width=640"));
        assert!(pipeline.contains("height=480"));
        assert!(pipeline.contains("nvarguscamerasrc"));
        assert!(pipeline.contains("appsink"));
        assert!(pipeline.contains("max-buffers=1"));
        assert!(pipeline.contains("drop=true"));
        assert!(pipeline.contains("leaky=downstream"));
    }

    #[test]
    fn camera_swap_reverses_sensor_ids() {
        let default_capture = GStreamerCapture::new(640, 480, false);
        assert!(default_capture.left_pipeline.contains("sensor-id=0"));
        assert!(default_capture.right_pipeline.contains("sensor-id=1"));

        let swapped_capture = GStreamerCapture::new(640, 480, true);
        assert!(swapped_capture.left_pipeline.contains("sensor-id=1"));
        assert!(swapped_capture.right_pipeline.contains("sensor-id=0"));
    }
}
