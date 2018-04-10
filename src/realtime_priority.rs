use thread_priority::*;

pub fn get_realtime_priority() {
    // attempt to acquire real-time priority
    let thread_id = thread_native_id();
    let policy = ThreadSchedulePolicy::Realtime(RealtimeThreadSchedulePolicy::Fifo);
    let params = ScheduleParams { sched_priority: 3 };
    set_thread_schedule_policy(thread_id, policy, params).unwrap();
}

