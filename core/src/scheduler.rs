use alloc::collections::BinaryHeap;
use core::cmp::Reverse;

use strum::EnumCount;

use crate::SystemTime;

#[derive(Debug, Clone)]
pub struct Scheduler {
    now: SystemTime,
    last_update: [SystemTime; Event::COUNT],
    events: BinaryHeap<Reverse<QueuedEvent>>,
}

#[derive(EnumCount, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Event {
    /// GPU and timers share elapsed time, video edges and dot clocks.
    Gpu,
    CdRom,
    Joy,
    Dma,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct QueuedEvent {
    deadline: SystemTime,
    event: Event,
}

impl Default for Scheduler {
    fn default() -> Self {
        let mut scheduler = Self {
            now: 0,
            last_update: [0; Event::COUNT],
            events: BinaryHeap::new(),
        };
        for event in [Event::Gpu, Event::Joy, Event::Dma] {
            scheduler.schedule(event, 0);
        }
        scheduler
    }
}

impl Scheduler {
    pub fn advance(&mut self, elapsed: SystemTime) {
        self.now = self
            .now
            .checked_add(elapsed)
            .expect("scheduler time overflow");
    }

    pub fn delay_till_next_event(&self) -> Option<SystemTime> {
        self.events
            .peek()
            .map(|entry| entry.0.deadline.saturating_sub(self.now))
    }

    pub fn schedule(&mut self, event: Event, delay: SystemTime) {
        self.events.push(Reverse(QueuedEvent {
            event,
            deadline: self
                .now
                .checked_add(delay)
                .expect("event deadline overflow"),
        }));
    }

    pub fn elapsed_since_update(&self, event: Event) -> SystemTime {
        self.now - self.last_update[event as usize]
    }

    pub fn take_elapsed(&mut self, event: Event) -> SystemTime {
        let elapsed = self.elapsed_since_update(event);
        self.last_update[event as usize] = self.now;
        elapsed
    }

    pub fn remove(&mut self, event: Event) {
        self.events.retain(|entry| entry.0.event != event);
    }

    pub fn pop(&mut self) -> Option<Event> {
        if self.events.peek()?.0.deadline > self.now {
            return None;
        }

        let Reverse(entry) = self.events.pop()?;
        Some(entry.event)
    }
}
