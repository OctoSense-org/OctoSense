//! Desktop Windows Hello consent is parented to the exact native review window.
//! The desktop interop API requires Windows build 22000 or later. Availability,
//! policy and cancellation failures never fall back to a script confirmation.
use super::{Cancel, PlatformCompletion, WindowTarget};
use makepad_widgets::{
    makepad_platform::os::windows::{win32_app::WIN32_APP, win32_window::Win32Window},
    Cx, SignalToUI, WindowId,
};
use std::{
    cell::OnceCell,
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    },
};
use webview2_windows::{
    core::{factory, HSTRING},
    Security::Credentials::UI::{
        UserConsentVerificationResult, UserConsentVerifier, UserConsentVerifierAvailability,
    },
    Win32::{
        Foundation::HWND,
        System::{
            Threading::GetCurrentThreadId,
            WinRT::{
                IUserConsentVerifierInterop, RoInitialize, RoUninitialize, RO_INIT_SINGLETHREADED,
            },
        },
        UI::WindowsAndMessaging::{
            GetWindowLongPtrW, GetWindowThreadProcessId, IsWindow, GWLP_USERDATA,
        },
    },
};
use windows_future::{AsyncOperationCompletedHandler, AsyncStatus, IAsyncOperation};

struct Apartment;
impl Drop for Apartment {
    fn drop(&mut self) {
        // This thread-local guard is dropped on the thread that initialized it.
        unsafe { RoUninitialize() };
    }
}
thread_local! {
    static APARTMENT: OnceCell<Apartment> = const { OnceCell::new() };
}

fn ensure_winrt() -> Result<(), String> {
    APARTMENT.with(|apartment| {
        if apartment.get().is_none() {
            unsafe { RoInitialize(RO_INIT_SINGLETHREADED) }
                .map_err(|_| "Windows verification cannot initialize on this thread")?;
            let _ = apartment.set(Apartment);
        }
        Ok(())
    })
}

/// Inspect only Makepad's own windows on its UI thread. Never use the active
/// desktop window, an app-supplied integer, or an HWND from another thread.
fn owned_handle(id: WindowId) -> Result<usize, String> {
    let handles = WIN32_APP
        .try_with(|app| {
            app.try_borrow()
                .ok()
                .and_then(|app| app.as_ref().map(|app| app.all_windows.clone()))
        })
        .ok()
        .flatten()
        .ok_or("The native review window is unavailable on this thread")?;
    for native in handles {
        let hwnd = HWND(native.0);
        // all_windows is maintained by Win32Window's UI-thread lifecycle. The
        // thread and IsWindow checks precede dereferencing its own USERDATA.
        if !unsafe { IsWindow(Some(hwnd)) }.as_bool()
            || unsafe { GetWindowThreadProcessId(hwnd, None) } != unsafe { GetCurrentThreadId() }
        {
            continue;
        }
        let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const Win32Window;
        let Some(window) = (unsafe { ptr.as_ref() }) else {
            continue;
        };
        if window.window_id == id && !window.is_closing.get() {
            return Ok(hwnd.0 as usize);
        }
    }
    Err("The exact native review window no longer exists".into())
}

pub(super) fn native_handle(cx: &Cx, id: WindowId) -> Result<usize, String> {
    if !cx.windows.is_valid(id) || !cx.windows[id].is_created {
        return Err("The review window is no longer available".into());
    }
    owned_handle(id)
}

fn verification_result(result: UserConsentVerificationResult) -> Result<(), String> {
    match result {
        UserConsentVerificationResult::Verified => Ok(()),
        UserConsentVerificationResult::DeviceNotPresent => {
            Err("Windows Hello verification is unavailable on this device".into())
        }
        UserConsentVerificationResult::NotConfiguredForUser => {
            Err("Set up Windows Hello in Windows Settings before approving this operation".into())
        }
        UserConsentVerificationResult::DisabledByPolicy => {
            Err("Windows policy has disabled Windows Hello verification".into())
        }
        UserConsentVerificationResult::DeviceBusy => {
            Err("Windows Hello is busy; review and try again".into())
        }
        UserConsentVerificationResult::RetriesExhausted => {
            Err("Windows Hello verification attempts were exhausted".into())
        }
        UserConsentVerificationResult::Canceled => Err("Windows verification was cancelled".into()),
        _ => Err("Windows did not verify this operation".into()),
    }
}

enum Operation {
    Availability(IAsyncOperation<UserConsentVerifierAvailability>),
    Verification(IAsyncOperation<UserConsentVerificationResult>),
}
impl Operation {
    fn cancel(&self) {
        match self {
            Self::Availability(operation) => {
                let _ = operation.Cancel();
            }
            Self::Verification(operation) => {
                let _ = operation.Cancel();
            }
        }
    }
}
struct Pending {
    target: WindowTarget,
    message: String,
    completion: PlatformCompletion,
    cancelled: Arc<AtomicBool>,
    operation: Operation,
}
fn pending() -> &'static Mutex<HashMap<String, Pending>> {
    static PENDING: OnceLock<Mutex<HashMap<String, Pending>>> = OnceLock::new();
    PENDING.get_or_init(|| Mutex::new(HashMap::new()))
}
fn retain(id: String, request: Pending) {
    // Cancel sets the flag before taking this same lock. It therefore cannot
    // miss a request temporarily removed while its UI-thread pump runs.
    let mut requests = pending().lock().unwrap_or_else(|e| e.into_inner());
    if !request.cancelled.load(Ordering::Acquire) {
        requests.insert(id, request);
        return;
    }
    drop(requests);
    request.operation.cancel();
}

pub(super) fn begin(
    target: WindowTarget,
    message: &str,
    completion: PlatformCompletion,
) -> Result<Cancel, String> {
    if completion.is_cancelled() {
        return Err("The review was already cancelled".into());
    }
    if target.native_handle == 0
        || owned_handle(WindowId(target.slot, target.generation))? != target.native_handle
    {
        return Err("The native review window changed; review again".into());
    }
    ensure_winrt()?;
    if pending().lock().unwrap_or_else(|e| e.into_inner()).len() >= 8 {
        return Err("Too many Windows verification requests are pending".into());
    }
    let operation = UserConsentVerifier::CheckAvailabilityAsync()
        .map_err(|_| "Windows Hello availability could not be checked")?;
    let ready = AsyncOperationCompletedHandler::<UserConsentVerifierAvailability>::new(|_, _| {
        SignalToUI::set_ui_signal();
        Ok(())
    });
    if operation.SetCompleted(&ready).is_err() {
        let _ = operation.Cancel();
        return Err("Windows Hello availability callback could not be registered".into());
    }
    let id = completion.challenge_id();
    let cancelled = Arc::new(AtomicBool::new(false));
    retain(
        id.clone(),
        Pending {
            target,
            message: message.to_owned(),
            completion,
            cancelled: cancelled.clone(),
            operation: Operation::Availability(operation),
        },
    );
    Ok(Box::new(move || {
        cancelled.store(true, Ordering::Release);
        let request = pending()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&id);
        if let Some(request) = request {
            // Native review normally cancels on its STA. If teardown moves to
            // another thread, initialize it before interacting with WinRT. The
            // common cancellation state has already made all late results inert.
            if ensure_winrt().is_ok() {
                request.operation.cancel();
            }
        }
    }))
}

/// Called on the UI thread only after common code revalidated this challenge's
/// app, bundle, account, review and window. An availability callback cannot open
/// a dialog, authorize an operation or pump any other challenge.
pub(super) fn pump(challenge_id: &str) {
    let Some(mut request) = pending()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(challenge_id)
    else {
        return;
    };
    if request.cancelled.load(Ordering::Acquire) || request.completion.is_cancelled() {
        request.operation.cancel();
        return;
    }
    let Operation::Availability(availability) = &request.operation else {
        retain(challenge_id.to_owned(), request);
        return;
    };
    match availability.Status() {
        Ok(AsyncStatus::Started) => {
            retain(challenge_id.to_owned(), request);
            return;
        }
        Ok(AsyncStatus::Completed) => {}
        _ => {
            request.completion.finish(Err(
                "Windows Hello availability check failed or was cancelled".into(),
            ));
            return;
        }
    }
    let available = availability_query_result(availability.GetResults());
    if let Err(error) = available {
        request.completion.finish(Err(error));
        return;
    }
    match request_verification(&request) {
        Ok(operation) => {
            request.operation = Operation::Verification(operation);
            retain(challenge_id.to_owned(), request);
        }
        Err(error) => request.completion.finish(Err(error)),
    }
}

fn availability_result(result: UserConsentVerifierAvailability) -> Result<(), String> {
    match result {
        UserConsentVerifierAvailability::Available => Ok(()),
        UserConsentVerifierAvailability::DeviceNotPresent => {
            Err("Windows Hello verification is unavailable on this device".into())
        }
        UserConsentVerifierAvailability::NotConfiguredForUser => {
            Err("Set up Windows Hello in Windows Settings before approving this operation".into())
        }
        UserConsentVerifierAvailability::DisabledByPolicy => {
            Err("Windows policy has disabled Windows Hello verification".into())
        }
        UserConsentVerifierAvailability::DeviceBusy => {
            Err("Windows Hello is busy; review and try again".into())
        }
        _ => Err("Windows Hello availability is unknown".into()),
    }
}

fn availability_query_result(
    result: webview2_windows::core::Result<UserConsentVerifierAvailability>,
) -> Result<(), String> {
    result
        .map_err(|_| "Windows Hello availability returned no valid result".to_owned())
        .and_then(availability_result)
}

fn request_verification(
    request: &Pending,
) -> Result<IAsyncOperation<UserConsentVerificationResult>, String> {
    ensure_winrt()?;
    let target = request.target;
    if owned_handle(WindowId(target.slot, target.generation))? != target.native_handle {
        return Err("The native review window changed; review again".into());
    }
    if request.cancelled.load(Ordering::Acquire) || request.completion.is_cancelled() {
        return Err("The review was cancelled".into());
    }
    let verifier: IUserConsentVerifierInterop =
        factory::<UserConsentVerifier, _>().map_err(|_| {
            "Desktop Windows verification is unavailable; Windows 11 or later is required"
        })?;
    let operation: IAsyncOperation<UserConsentVerificationResult> = unsafe {
        verifier.RequestVerificationForWindowAsync(
            HWND(target.native_handle as *mut _),
            &HSTRING::from(request.message.as_str()),
        )
    }
    .map_err(|_| "Windows could not start verification for the review window")?;
    // Capture no operation, avoiding a COM reference cycle. Common state ignores
    // late callbacks and consumes successful evidence only once after rechecking.
    let completion = request.completion.clone();
    let handler = AsyncOperationCompletedHandler::<UserConsentVerificationResult>::new(
        move |sender, status| {
            if completion.is_cancelled() {
                return Ok(());
            }
            let outcome = if status == AsyncStatus::Completed {
                sender
                    .as_ref()
                    .ok_or_else(|| "Windows verification returned no operation".to_owned())
                    .and_then(|operation| {
                        operation
                            .GetResults()
                            .map_err(|_| "Windows verification returned no valid result".to_owned())
                    })
                    .and_then(verification_result)
            } else if status == AsyncStatus::Canceled {
                Err("Windows verification was cancelled".into())
            } else {
                Err("Windows verification failed".into())
            };
            completion.finish(outcome);
            Ok(())
        },
    );
    if operation.SetCompleted(&handler).is_err() {
        let _ = operation.Cancel();
        return Err("Windows verification could not register its completion handler".into());
    }
    Ok(operation)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unavailable_request() -> (String, Pending) {
        let completion = PlatformCompletion {
            state: Arc::new(Mutex::new(super::super::State::Waiting)),
            nonce: uuid::Uuid::new_v4(),
            digest: [0; 32],
        };
        (
            completion.challenge_id(),
            Pending {
                target: WindowTarget {
                    slot: 0,
                    generation: 0,
                    native_handle: 0,
                },
                message: "Native adapter test".into(),
                completion,
                cancelled: Arc::new(AtomicBool::new(false)),
                operation: Operation::Availability(IAsyncOperation::ready(Ok(
                    UserConsentVerifierAvailability::DeviceNotPresent,
                ))),
            },
        )
    }

    #[test]
    fn availability_pump_processes_only_its_validated_challenge() {
        let (first, first_request) = unavailable_request();
        let (second, second_request) = unavailable_request();
        let first_result = first_request.completion.clone();
        let second_result = second_request.completion.clone();
        retain(first.clone(), first_request);
        retain(second.clone(), second_request);
        pump(&first);
        assert!(matches!(
            *first_result.state.lock().unwrap(),
            super::super::State::Finished(Err(_))
        ));
        assert!(matches!(
            *second_result.state.lock().unwrap(),
            super::super::State::Waiting
        ));
        assert!(!pending().lock().unwrap().contains_key(&first));
        pump(&second);
        assert!(!pending().lock().unwrap().contains_key(&second));
    }

    #[test]
    fn cancellation_while_removed_for_pump_cannot_retain_request() {
        let (id, request) = unavailable_request();
        let cancelled = request.cancelled.clone();
        retain(id.clone(), request);
        let request = pending().lock().unwrap().remove(&id).unwrap();
        // Cancellation can occur while pump owns the request outside the lock.
        cancelled.store(true, Ordering::Release);
        retain(id.clone(), request);
        assert!(!pending().lock().unwrap().contains_key(&id));
    }

    #[test]
    fn only_native_verified_authorizes() {
        assert!(verification_result(UserConsentVerificationResult::Verified).is_ok());
        for value in 1..=6 {
            assert!(verification_result(UserConsentVerificationResult(value)).is_err());
        }
        assert!(verification_result(UserConsentVerificationResult(-1)).is_err());
        assert!(verification_result(UserConsentVerificationResult(99)).is_err());
    }

    #[test]
    fn unavailable_or_unknown_never_opens_verification() {
        assert!(availability_result(UserConsentVerifierAvailability::Available).is_ok());
        for value in 1..=4 {
            assert!(availability_result(UserConsentVerifierAvailability(value)).is_err());
        }
        assert!(availability_result(UserConsentVerifierAvailability(-1)).is_err());
        assert!(availability_result(UserConsentVerifierAvailability(99)).is_err());
    }

    /// Opt in on a real Windows runner. This checks native availability only;
    /// it never calls RequestVerificationForWindowAsync or authenticates anyone.
    #[test]
    #[ignore = "requires native Windows WinRT; availability only, no Hello prompt"]
    fn native_windows_hello_availability_without_prompt() {
        use std::time::{Duration, Instant};
        use webview2_windows::Win32::UI::WindowsAndMessaging::{
            DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE,
        };
        ensure_winrt().expect("Initialize the availability probe STA");
        let operation = match UserConsentVerifier::CheckAvailabilityAsync() {
            Ok(operation) => operation,
            Err(error) => {
                let code = error.code().0;
                assert!(availability_query_result(Err(error)).is_err());
                println!(
                    "WINDOWS_HELLO_AVAILABILITY query_error={code} authentication_attempted=false"
                );
                return;
            }
        };
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            match operation.Status() {
                Ok(AsyncStatus::Started) => {}
                Ok(AsyncStatus::Completed) => {
                    let result = operation.GetResults();
                    let available = result
                        .as_ref()
                        .is_ok_and(|value| *value == UserConsentVerifierAvailability::Available);
                    let code = result.as_ref().map(|value| value.0).unwrap_or(-1);
                    assert_eq!(availability_query_result(result).is_ok(), available);
                    println!("WINDOWS_HELLO_AVAILABILITY result={code} available={available} authentication_attempted=false");
                    return;
                }
                Ok(status) => {
                    assert_ne!(status, AsyncStatus::Completed);
                    println!("WINDOWS_HELLO_AVAILABILITY terminal_status={} authentication_attempted=false", status.0);
                    return;
                }
                Err(error) => {
                    let code = error.code().0;
                    assert!(availability_query_result(Err(error)).is_err());
                    println!("WINDOWS_HELLO_AVAILABILITY status_error={code} authentication_attempted=false");
                    let _ = operation.Cancel();
                    return;
                }
            }
            if Instant::now() >= deadline {
                let _ = operation.Cancel();
                panic!("Native Windows Hello availability did not finish within 15 seconds");
            }
            // The bounded STA pump allows native completion messages through
            // without creating a test window or showing any verification UI.
            let mut message = MSG::default();
            for _ in 0..64 {
                if !unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE) }.as_bool() {
                    break;
                }
                unsafe {
                    let _ = TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn missing_owned_ui_window_is_refused() {
        assert!(owned_handle(WindowId(0, 0)).is_err());
    }
}
