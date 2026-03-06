//! Self-test clip registry and test infrastructure.
//!
//! Defines the set of test clips used to verify the video transcoding pipeline.

pub mod runner;
pub mod validate;

use serde::Serialize;

/// Category of test clip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TestCategory {
    Standard,
    EdgeCase,
}

/// Controls which subset of clips to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TestMode {
    Quick,
    Full,
}

impl TestMode {
    pub fn parse_mode(s: &str) -> Option<Self> {
        match s {
            "quick" => Some(Self::Quick),
            "full" => Some(Self::Full),
            _ => None,
        }
    }
}

/// A test clip definition with expected properties.
pub struct TestClip {
    pub name: &'static str,
    pub filename: &'static str,
    pub expected_codec: &'static str,
    pub expected_width: u32,
    pub expected_height: u32,
    pub has_audio: bool,
    pub category: TestCategory,
}

/// Registry of all test clips.
pub const TEST_CLIPS: &[TestClip] = &[
    // --- Standard clips ---
    TestClip {
        name: "h264_1080p",
        filename: "h264-1080p-landscape.mp4",
        expected_codec: "h264",
        expected_width: 1920,
        expected_height: 1080,
        has_audio: true,
        category: TestCategory::Standard,
    },
    TestClip {
        name: "hevc_1080p",
        filename: "hevc-1080p-landscape.mp4",
        expected_codec: "hevc",
        expected_width: 1920,
        expected_height: 1080,
        has_audio: true,
        category: TestCategory::Standard,
    },
    TestClip {
        name: "av1_1080p",
        filename: "av1-1080p-landscape.mp4",
        expected_codec: "av1",
        expected_width: 1920,
        expected_height: 1080,
        has_audio: true,
        category: TestCategory::Standard,
    },
    TestClip {
        name: "h264_4k",
        filename: "h264-4k-landscape.mp4",
        expected_codec: "h264",
        expected_width: 3840,
        expected_height: 2160,
        has_audio: true,
        category: TestCategory::Standard,
    },
    // --- Edge case clips ---
    TestClip {
        name: "vp9_1080p",
        filename: "vp9-1080p-landscape.webm",
        expected_codec: "vp9",
        expected_width: 1920,
        expected_height: 1080,
        has_audio: true,
        category: TestCategory::EdgeCase,
    },
    TestClip {
        name: "h264_portrait",
        filename: "h264-1080p-portrait.mp4",
        expected_codec: "h264",
        expected_width: 1080,
        expected_height: 1920,
        has_audio: true,
        category: TestCategory::EdgeCase,
    },
    TestClip {
        name: "h264_noaudio",
        filename: "h264-noaudio-720p.mp4",
        expected_codec: "h264",
        expected_width: 1280,
        expected_height: 720,
        has_audio: false,
        category: TestCategory::EdgeCase,
    },
    TestClip {
        name: "h264_240p",
        filename: "h264-240p-landscape.mp4",
        expected_codec: "h264",
        expected_width: 426,
        expected_height: 240,
        has_audio: true,
        category: TestCategory::EdgeCase,
    },
    TestClip {
        name: "vp9_720p",
        filename: "vp9-720p-landscape.webm",
        expected_codec: "vp9",
        expected_width: 1280,
        expected_height: 720,
        has_audio: true,
        category: TestCategory::EdgeCase,
    },
    TestClip {
        name: "hevc_720p",
        filename: "hevc-720p-landscape.mp4",
        expected_codec: "hevc",
        expected_width: 1280,
        expected_height: 720,
        has_audio: true,
        category: TestCategory::EdgeCase,
    },
    TestClip {
        name: "av1_720p",
        filename: "av1-720p-landscape.mp4",
        expected_codec: "av1",
        expected_width: 1280,
        expected_height: 720,
        has_audio: true,
        category: TestCategory::EdgeCase,
    },
];

/// Returns the clips applicable for the given test mode.
///
/// - `Quick` returns only `Standard` clips.
/// - `Full` returns all clips.
pub fn clips_for_mode(mode: TestMode) -> Vec<&'static TestClip> {
    match mode {
        TestMode::Quick => TEST_CLIPS
            .iter()
            .filter(|c| c.category == TestCategory::Standard)
            .collect(),
        TestMode::Full => TEST_CLIPS.iter().collect(),
    }
}
