use boltffi::*;
use crate::Counter;

#[export]
pub fn describe_counter(counter: &Counter) -> String {
    format!("Counter(value={})", counter.get())
}
