use regex::Regex;

pub struct DkimEntry {
    pub key: String,
    pub value: String,
}

pub fn parse(dkim_entry: &str) -> anyhow::Result<DkimEntry> {
    let key = dkim_entry
        .split_whitespace()
        .next()
        .ok_or_else(|| anyhow::anyhow!("DKIM entry is empty"))?;

    let mut value = String::new();
    let re = Regex::new(r#""(.*?)""#).unwrap();
    for cap in re.captures_iter(dkim_entry) {
        value.push_str(&cap[0].trim_end_matches("\"").trim_start_matches("\""));
    }

    Ok(DkimEntry {
        key: key.to_owned(),
        value,
    })
}
