pub(crate) fn get(path: &str) -> Option<&'static [u8]> {
    match path {
        "/" => Some(include_bytes!("../web/index.html")),
        "/assets/app.js" => Some(include_bytes!("../web/assets/app.js")),
        "/assets/app.css" => Some(include_bytes!("../web/assets/app.css")),
        _ => None,
    }
}
