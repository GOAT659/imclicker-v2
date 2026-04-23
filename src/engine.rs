use crate::config::{AppConfig, BindMode};
use std::sync::{
    atomic::{AtomicBool, AtomicU16, AtomicU32, AtomicU64, AtomicU8, Ordering},
    Arc,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const MAX_CPS: u32 = 1000;
const SPIN_THRESHOLD: Duration = Duration::from_micros(220);
const MAX_COARSE_SLICE: Duration = Duration::from_millis(2);

pub struct SharedState {
    pub target_cps: AtomicU32,
    pub mode: AtomicU8,
    pub bind_vk: AtomicU16,
    pub manual_active: AtomicBool,
    pub active: AtomicBool,
    pub live_cps_x10: AtomicU32,
    pub total_clicks: AtomicU64,
    pub worker_alive: AtomicBool,
    shutdown: AtomicBool,
}

impl SharedState {
    pub fn new(config: &AppConfig) -> Arc<Self> {
        Arc::new(Self {
            target_cps: AtomicU32::new(config.target_cps.clamp(1, MAX_CPS)),
            mode: AtomicU8::new(config.mode.as_u8()),
            bind_vk: AtomicU16::new(config.bind_vk),
            manual_active: AtomicBool::new(config.manual_active),
            active: AtomicBool::new(false),
            live_cps_x10: AtomicU32::new(0),
            total_clicks: AtomicU64::new(0),
            worker_alive: AtomicBool::new(false),
            shutdown: AtomicBool::new(false),
        })
    }

    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Relaxed);
    }

    pub fn is_shutdown(&self) -> bool {
        self.shutdown.load(Ordering::Relaxed)
    }
}

pub struct ClickEngine {
    shared: Arc<SharedState>,
    join_handle: Option<JoinHandle<()>>,
}

impl ClickEngine {
    pub fn spawn(shared: Arc<SharedState>) -> Self {
        let thread_shared = Arc::clone(&shared);
        let join_handle = thread::Builder::new()
            .name("imclicker_v2_engine".into())
            .spawn(move || worker_loop(thread_shared))
            .expect("failed to spawn click engine thread");

        Self {
            shared,
            join_handle: Some(join_handle),
        }
    }
}

impl Drop for ClickEngine {
    fn drop(&mut self) {
        self.shared.shutdown();

        if let Some(handle) = self.join_handle.take() {
            let _ = handle.join();
        }
    }
}

fn worker_loop(shared: Arc<SharedState>) {
    shared.worker_alive.store(true, Ordering::Relaxed);
    elevate_worker_priority();

    let sleeper = HighResSleeper::new();
    let mut next_deadline = Instant::now();
    let mut hotkey_toggle_active = false;
    let mut prev_bind_down = false;

    let mut window_start = Instant::now();
    let mut window_clicks: u32 = 0;

    while !shared.is_shutdown() {
        let cps = shared.target_cps.load(Ordering::Relaxed).clamp(1, MAX_CPS);
        let mode = BindMode::from_u8(shared.mode.load(Ordering::Relaxed));
        let bind_vk = shared.bind_vk.load(Ordering::Relaxed);
        let manual_active = shared.manual_active.load(Ordering::Relaxed);
        let bind_down = is_key_down(bind_vk);

        match mode {
            BindMode::Toggle => {
                if bind_down && !prev_bind_down {
                    hotkey_toggle_active = !hotkey_toggle_active;
                }
            }
            BindMode::Hold => {}
        }
        prev_bind_down = bind_down;

        let hotkey_active = match mode {
            BindMode::Toggle => hotkey_toggle_active,
            BindMode::Hold => bind_down,
        };

        let is_active = manual_active || hotkey_active;
        shared.active.store(is_active, Ordering::Relaxed);

        if is_active {
            let period = cps_to_period(cps);
            let now = Instant::now();

            if next_deadline + period < now {
                next_deadline = now;
            }

            if !wait_until(&sleeper, next_deadline, &shared, hotkey_toggle_active) {
                next_deadline = Instant::now();
                update_live_cps(&shared, &mut window_start, &mut window_clicks);
                continue;
            }

            send_left_click();
            shared.total_clicks.fetch_add(1, Ordering::Relaxed);
            window_clicks = window_clicks.saturating_add(1);

            next_deadline += period;
            let corrected_now = Instant::now();
            if next_deadline <= corrected_now {
                next_deadline = corrected_now + period;
            }
        } else {
            next_deadline = Instant::now();
            sleep_for(&sleeper, Duration::from_millis(2));
        }

        update_live_cps(&shared, &mut window_start, &mut window_clicks);
    }

    shared.active.store(false, Ordering::Relaxed);
    shared.live_cps_x10.store(0, Ordering::Relaxed);
    shared.worker_alive.store(false, Ordering::Relaxed);
}

fn update_live_cps(shared: &SharedState, window_start: &mut Instant, window_clicks: &mut u32) {
    let elapsed = window_start.elapsed();
    if elapsed >= Duration::from_millis(300) {
        let cps_x10 = if *window_clicks == 0 {
            0
        } else {
            ((*window_clicks as f64 / elapsed.as_secs_f64()) * 10.0).round() as u32
        };

        shared.live_cps_x10.store(cps_x10, Ordering::Relaxed);
        *window_start = Instant::now();
        *window_clicks = 0;
    }
}

fn cps_to_period(cps: u32) -> Duration {
    Duration::from_nanos(1_000_000_000u64 / cps.max(1) as u64)
}

fn wait_until(
    sleeper: &Option<HighResSleeper>,
    deadline: Instant,
    shared: &SharedState,
    hotkey_toggle_active: bool,
) -> bool {
    loop {
        if shared.is_shutdown() {
            return false;
        }

        let mode = BindMode::from_u8(shared.mode.load(Ordering::Relaxed));
        let bind_vk = shared.bind_vk.load(Ordering::Relaxed);
        let bind_down = is_key_down(bind_vk);
        let hotkey_active = match mode {
            BindMode::Toggle => hotkey_toggle_active,
            BindMode::Hold => bind_down,
        };
        let still_active = shared.manual_active.load(Ordering::Relaxed) || hotkey_active;

        if !still_active {
            return false;
        }

        let now = Instant::now();
        if now >= deadline {
            return true;
        }

        let remaining = deadline - now;
        if remaining <= SPIN_THRESHOLD {
            break;
        }

        let coarse = (remaining - SPIN_THRESHOLD).min(MAX_COARSE_SLICE);
        sleep_for(sleeper, coarse);
    }

    while Instant::now() < deadline {
        if shared.is_shutdown() {
            return false;
        }

        let mode = BindMode::from_u8(shared.mode.load(Ordering::Relaxed));
        let bind_vk = shared.bind_vk.load(Ordering::Relaxed);
        let bind_down = is_key_down(bind_vk);
        let hotkey_active = match mode {
            BindMode::Toggle => hotkey_toggle_active,
            BindMode::Hold => bind_down,
        };
        let still_active = shared.manual_active.load(Ordering::Relaxed) || hotkey_active;

        if !still_active {
            return false;
        }

        std::hint::spin_loop();
    }

    true
}

fn sleep_for(sleeper: &Option<HighResSleeper>, duration: Duration) {
    if duration.is_zero() {
        return;
    }

    if let Some(sleeper) = sleeper {
        sleeper.sleep(duration);
    } else {
        thread::sleep(duration);
    }
}

#[cfg(windows)]
fn elevate_worker_priority() {
    use windows_sys::Win32::System::Threading::{
        GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_HIGHEST,
    };

    unsafe {
        let thread = GetCurrentThread();
        let _ = SetThreadPriority(thread, THREAD_PRIORITY_HIGHEST);
    }
}

#[cfg(not(windows))]
fn elevate_worker_priority() {}

#[cfg(windows)]
fn is_key_down(vk: u16) -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;

    unsafe { (GetAsyncKeyState(vk as i32) as u16 & 0x8000) != 0 }
}

#[cfg(not(windows))]
fn is_key_down(_vk: u16) -> bool {
    false
}

#[cfg(windows)]
fn send_left_click() {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
        MOUSEINPUT,
    };

    unsafe {
        let mut inputs = [
            INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        dx: 0,
                        dy: 0,
                        mouseData: 0,
                        dwFlags: MOUSEEVENTF_LEFTDOWN,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
            INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        dx: 0,
                        dy: 0,
                        mouseData: 0,
                        dwFlags: MOUSEEVENTF_LEFTUP,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
        ];

        let _ = SendInput(
            inputs.len() as u32,
            inputs.as_mut_ptr(),
            std::mem::size_of::<INPUT>() as i32,
        );
    }
}

#[cfg(not(windows))]
fn send_left_click() {}

#[cfg(windows)]
struct HighResSleeper {
    timer: windows_sys::Win32::Foundation::HANDLE,
}

#[cfg(windows)]
impl HighResSleeper {
    fn new() -> Option<Self> {
        use windows_sys::Win32::System::Threading::CreateWaitableTimerExW;

        const CREATE_WAITABLE_TIMER_HIGH_RESOLUTION: u32 = 0x0000_0002;
        const TIMER_ALL_ACCESS: u32 = 0x001F_0003;

        unsafe {
            let timer = CreateWaitableTimerExW(
                std::ptr::null(),
                std::ptr::null(),
                CREATE_WAITABLE_TIMER_HIGH_RESOLUTION,
                TIMER_ALL_ACCESS,
            );

            if timer.is_null() {
                None
            } else {
                Some(Self { timer })
            }
        }
    }

    fn sleep(&self, duration: Duration) {
        use windows_sys::Win32::System::Threading::{SetWaitableTimerEx, WaitForSingleObject};

        const INFINITE: u32 = 0xFFFF_FFFF;

        let intervals_100ns = (duration.as_nanos() / 100) as i64;
        if intervals_100ns <= 0 {
            return;
        }

        let due_time: i64 = -intervals_100ns;

        unsafe {
            let _ = SetWaitableTimerEx(
                self.timer,
                &due_time,
                0,
                None,
                std::ptr::null_mut(),
                std::ptr::null(),
                0,
            );
            let _ = WaitForSingleObject(self.timer, INFINITE);
        }
    }
}

#[cfg(windows)]
impl Drop for HighResSleeper {
    fn drop(&mut self) {
        use windows_sys::Win32::Foundation::CloseHandle;

        unsafe {
            let _ = CloseHandle(self.timer);
        }
    }
}

#[cfg(not(windows))]
struct HighResSleeper;

#[cfg(not(windows))]
impl HighResSleeper {
    fn new() -> Option<Self> {
        None
    }

    fn sleep(&self, duration: Duration) {
        thread::sleep(duration);
    }
}
