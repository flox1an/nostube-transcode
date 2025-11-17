use nostr::Event;

/// Extract the input URL from a DVM request event
pub fn get_input_url(event: &Event) -> Option<String> {
    event
        .tags
        .iter()
        .find(|tag| {
            tag.as_vec().len() >= 3
                && tag.as_vec()[0] == "i"
                && tag.as_vec()[2] == "url"
        })
        .and_then(|tag| tag.as_vec().get(1).map(|s| s.to_string()))
}

/// Extract relay hints from a DVM request event
pub fn get_relay_hints(event: &Event) -> Vec<String> {
    event
        .tags
        .iter()
        .find(|tag| tag.as_vec().len() >= 2 && tag.as_vec()[0] == "relays")
        .map(|tag| {
            tag.as_vec()
                .iter()
                .skip(1)
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// Extract output format from a DVM request event
pub fn get_output_format(event: &Event) -> Option<String> {
    event
        .tags
        .iter()
        .find(|tag| tag.as_vec().len() >= 2 && tag.as_vec()[0] == "output")
        .and_then(|tag| tag.as_vec().get(1).map(|s| s.to_string()))
}

/// Extract custom parameters from a DVM request event
pub fn get_params(event: &Event) -> Vec<(String, String)> {
    event
        .tags
        .iter()
        .filter(|tag| tag.as_vec().len() >= 3 && tag.as_vec()[0] == "param")
        .filter_map(|tag| {
            let vec = tag.as_vec();
            if vec.len() >= 3 {
                Some((vec[1].to_string(), vec[2].to_string()))
            } else {
                None
            }
        })
        .collect()
}

/// Check if event is encrypted
pub fn is_encrypted(event: &Event) -> bool {
    event
        .tags
        .iter()
        .any(|tag| tag.as_vec().len() >= 1 && tag.as_vec()[0] == "encrypted")
}

#[derive(Debug, Clone, PartialEq)]
pub enum OutputFormat {
    Hls,
    Mp4,
}

impl OutputFormat {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "hls" => Some(OutputFormat::Hls),
            "mp4" => Some(OutputFormat::Mp4),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            OutputFormat::Hls => "hls",
            OutputFormat::Mp4 => "mp4",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Resolution {
    R480p,
    R720p,
    R1080p,
}

impl Resolution {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "480p" => Some(Resolution::R480p),
            "720p" => Some(Resolution::R720p),
            "1080p" => Some(Resolution::R1080p),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Resolution::R480p => "480p",
            Resolution::R720p => "720p",
            Resolution::R1080p => "1080p",
        }
    }

    pub fn width(&self) -> u32 {
        match self {
            Resolution::R480p => 854,
            Resolution::R720p => 1280,
            Resolution::R1080p => 1920,
        }
    }

    pub fn height(&self) -> u32 {
        match self {
            Resolution::R480p => 480,
            Resolution::R720p => 720,
            Resolution::R1080p => 1080,
        }
    }
}

/// Extract output format from parameters (defaults to HLS)
pub fn get_output_format_from_params(event: &Event) -> OutputFormat {
    let params = get_params(event);

    // Check for "output" parameter
    for (key, value) in params {
        if key == "output" {
            if let Some(format) = OutputFormat::from_str(&value) {
                return format;
            }
        }
    }

    // Also check the legacy "output" tag
    if let Some(output) = get_output_format(event) {
        if let Some(format) = OutputFormat::from_str(&output) {
            return format;
        }
    }

    // Default to HLS
    OutputFormat::Hls
}

/// Extract resolutions from parameters (defaults to all if not specified)
pub fn get_resolutions_from_params(event: &Event) -> Vec<Resolution> {
    let params = get_params(event);

    let mut resolutions = Vec::new();
    for (key, value) in params {
        if key == "resolution" {
            if let Some(res) = Resolution::from_str(&value) {
                if !resolutions.contains(&res) {
                    resolutions.push(res);
                }
            }
        }
    }

    // If no resolutions specified, default to all
    if resolutions.is_empty() {
        resolutions = vec![Resolution::R480p, Resolution::R720p, Resolution::R1080p];
    }

    resolutions
}
