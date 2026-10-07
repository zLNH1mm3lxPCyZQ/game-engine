use std::marker::PhantomData;

use crate::resource::Resource;

struct Buffer<E> {
    /// Number of the first event in `events`.
    start: u64,
    events: Vec<E>,
}

/// A queue of messages of type `E`, stored as a resource.
/// Every event survives two `update` calls, then is dropped.
pub struct Events<E> {
    previous: Buffer<E>,
    current: Buffer<E>,
}

impl<E> Default for Events<E> {
    fn default() -> Self {
        Self {
            previous: Buffer {
                start: 0,
                events: Vec::new(),
            },
            current: Buffer {
                start: 0,
                events: Vec::new(),
            },
        }
    }
}

impl<E: Resource> Events<E> {
    pub fn send(&mut self, event: E) {
        self.current.events.push(event);
    }

    /// Drop the oldest buffer of events. Call once per frame.
    pub fn update(&mut self) {
        let next = self.next_number();
        let current = std::mem::replace(
            &mut self.current,
            Buffer {
                start: next,
                events: Vec::new(),
            },
        );
        self.previous = current;
    }

    /// Number that the next sent event will get.
    fn next_number(&self) -> u64 {
        self.current.start + self.current.events.len() as u64
    }
}

/// A reader's position in an event queue: it never sees the same event twice.
pub struct EventCursor<E> {
    next: u64,
    _marker: PhantomData<fn() -> E>,
}

impl<E> Default for EventCursor<E> {
    fn default() -> Self {
        Self {
            next: 0,
            _marker: PhantomData,
        }
    }
}

impl<E: Resource> EventCursor<E> {
    /// Events sent since this cursor last read, oldest first.
    pub fn read<'a>(&mut self, events: &'a Events<E>) -> impl Iterator<Item = &'a E> + use<'a, E> {
        let from = self.next;
        self.next = events.next_number();
        [&events.previous, &events.current]
            .into_iter()
            .flat_map(move |buffer| {
                buffer
                    .events
                    .iter()
                    .skip(from.saturating_sub(buffer.start) as usize)
            })
    }
}
