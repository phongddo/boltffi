use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::Duration;

use crate::ringbuffer::SpscRingBuffer;

pub struct EventSubscription<T: Send + 'static> {
    ring_buffer: SpscRingBuffer<T>,
    is_active: AtomicBool,
    notification_mutex: Mutex<()>,
    notification_condvar: Condvar,
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitResult {
    EventsAvailable = 1,
    Timeout = 0,
    Unsubscribed = -1,
}

impl<T: Send + 'static> EventSubscription<T> {
    pub fn new(capacity: usize) -> Self {
        Self {
            ring_buffer: SpscRingBuffer::new(capacity),
            is_active: AtomicBool::new(true),
            notification_mutex: Mutex::new(()),
            notification_condvar: Condvar::new(),
        }
    }

    pub fn is_active(&self) -> bool {
        self.is_active.load(Ordering::Acquire)
    }

    pub fn push_event(&self, event: T) -> bool {
        if !self.is_active() {
            return false;
        }

        let push_succeeded = self.ring_buffer.push(event).is_ok();

        if push_succeeded {
            self.notification_condvar.notify_one();
        }

        push_succeeded
    }

    pub fn pop_event(&self) -> Option<T> {
        self.ring_buffer.pop()
    }

    pub fn pop_batch_into(&self, output_buffer: &mut [std::mem::MaybeUninit<T>]) -> usize {
        self.ring_buffer.pop_batch_into(output_buffer)
    }

    pub fn wait_for_events(&self, timeout_milliseconds: u32) -> WaitResult {
        if !self.is_active() {
            return WaitResult::Unsubscribed;
        }

        if self.ring_buffer.available_count() > 0 {
            return WaitResult::EventsAvailable;
        }

        let notification_guard = self.notification_mutex.lock().unwrap();
        let timeout_duration = Duration::from_millis(timeout_milliseconds as u64);

        let wait_result = self
            .notification_condvar
            .wait_timeout_while(notification_guard, timeout_duration, |_| {
                self.is_active() && self.ring_buffer.is_empty()
            });

        if !self.is_active() {
            return WaitResult::Unsubscribed;
        }

        match wait_result {
            Ok((_, timeout_result)) if timeout_result.timed_out() => WaitResult::Timeout,
            _ => {
                if self.ring_buffer.available_count() > 0 {
                    WaitResult::EventsAvailable
                } else {
                    WaitResult::Timeout
                }
            }
        }
    }

    pub fn unsubscribe(&self) {
        self.is_active.store(false, Ordering::Release);
        self.notification_condvar.notify_all();
    }

    pub fn available_count(&self) -> usize {
        self.ring_buffer.available_count()
    }
}

impl<T: Send + 'static> Drop for EventSubscription<T> {
    fn drop(&mut self) {
        self.unsubscribe();
    }
}

pub type SubscriptionHandle = *mut core::ffi::c_void;

pub fn subscription_new<T: Send + 'static>(capacity: usize) -> SubscriptionHandle {
    let subscription = Box::new(EventSubscription::<T>::new(capacity));
    Box::into_raw(subscription) as SubscriptionHandle
}

pub unsafe fn subscription_push<T: Send + 'static>(
    handle: SubscriptionHandle,
    event: T,
) -> bool {
    if handle.is_null() {
        return false;
    }
    let subscription = unsafe { &*(handle as *const EventSubscription<T>) };
    subscription.push_event(event)
}

pub unsafe fn subscription_pop_batch<T: Send + Copy + 'static>(
    handle: SubscriptionHandle,
    output_ptr: *mut T,
    output_capacity: usize,
) -> usize {
    if handle.is_null() || output_ptr.is_null() || output_capacity == 0 {
        return 0;
    }

    let subscription = unsafe { &*(handle as *const EventSubscription<T>) };
    let output_slice = unsafe {
        std::slice::from_raw_parts_mut(
            output_ptr as *mut std::mem::MaybeUninit<T>,
            output_capacity,
        )
    };

    subscription.pop_batch_into(output_slice)
}

pub unsafe fn subscription_wait<T: Send + 'static>(
    handle: SubscriptionHandle,
    timeout_milliseconds: u32,
) -> i32 {
    if handle.is_null() {
        return WaitResult::Unsubscribed as i32;
    }

    let subscription = unsafe { &*(handle as *const EventSubscription<T>) };
    subscription.wait_for_events(timeout_milliseconds) as i32
}

pub unsafe fn subscription_unsubscribe<T: Send + 'static>(handle: SubscriptionHandle) {
    if handle.is_null() {
        return;
    }

    let subscription = unsafe { &*(handle as *const EventSubscription<T>) };
    subscription.unsubscribe();
}

pub unsafe fn subscription_free<T: Send + 'static>(handle: SubscriptionHandle) {
    if handle.is_null() {
        return;
    }

    let subscription = unsafe { Box::from_raw(handle as *mut EventSubscription<T>) };
    drop(subscription);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_subscription_push_pop() {
        let subscription = EventSubscription::<i32>::new(16);
        assert!(subscription.push_event(42));
        assert!(subscription.push_event(100));
        assert_eq!(subscription.pop_event(), Some(42));
        assert_eq!(subscription.pop_event(), Some(100));
        assert_eq!(subscription.pop_event(), None);
    }

    #[test]
    fn test_subscription_unsubscribe_stops_push() {
        let subscription = EventSubscription::<i32>::new(16);
        assert!(subscription.push_event(1));
        subscription.unsubscribe();
        assert!(!subscription.push_event(2));
        assert!(!subscription.is_active());
    }

    #[test]
    fn test_subscription_wait_immediate_return() {
        let subscription = EventSubscription::<i32>::new(16);
        subscription.push_event(42);
        assert_eq!(
            subscription.wait_for_events(1000),
            WaitResult::EventsAvailable
        );
    }

    #[test]
    fn test_subscription_wait_timeout() {
        let subscription = EventSubscription::<i32>::new(16);
        assert_eq!(subscription.wait_for_events(10), WaitResult::Timeout);
    }

    #[test]
    fn test_subscription_cross_thread() {
        use std::sync::Arc;

        let subscription = Arc::new(EventSubscription::<i32>::new(1024));
        let producer_subscription = Arc::clone(&subscription);

        let producer_thread = thread::spawn(move || {
            (0..100).for_each(|index| {
                producer_subscription.push_event(index);
                thread::sleep(Duration::from_micros(100));
            });
        });

        let mut received_events = Vec::new();
        while received_events.len() < 100 {
            let wait_result = subscription.wait_for_events(100);
            if wait_result == WaitResult::Unsubscribed {
                break;
            }

            while let Some(event) = subscription.pop_event() {
                received_events.push(event);
            }
        }

        producer_thread.join().unwrap();
        assert_eq!(received_events.len(), 100);
        assert!(received_events.iter().enumerate().all(|(index, &value)| value == index as i32));
    }
}
