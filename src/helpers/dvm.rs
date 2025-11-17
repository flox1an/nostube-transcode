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
