pub fn clean_string(s: &str) -> String {
    return s.replace(r"\", r"\\").replace("'", r"\'");
}

pub fn bool_to_sql(val: bool) -> String {
    if val {
        return "1".to_string();
    } else {
        return "0".to_string();
    }
}
