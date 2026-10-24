use crate::AppError;
use serde_json::Value;
pub type Result<T> = std::result::Result<T,AppError>;
pub fn check(ok:bool,code:&'static str,message:impl Into<String>)->Result<()> {
    if ok { Ok(()) } else {Err(AppError::new(code,message))}
}
pub fn number(v:&Value)->f64 {v.as_f64().unwrap_or(0.0)}
pub fn merge(mut a:Value,b:Value)->Value {
    if let (Some(a),Some(b))=(a.as_object_mut(),b.as_object()) {for (k,v) in b {a.insert(k.clone(),v.clone());}}
    a
}
