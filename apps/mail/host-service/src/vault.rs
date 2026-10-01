//! Where the mail service keeps passwords: the platform's own secret store.
//!
//! - macOS and iOS: the keychain, one item per account.
//! - Android: a file per account under the service's directory, encrypted
//!   with an AES key that lives in the Android Keystore and never leaves it.
//! - Elsewhere: an owner-only file under the service's directory.
//!
//! The files live in the host's secrets folder, `<home>/secrets/os.mail/`
//! (0700, files 0600; ADR 0004 §11), when the host names it
//! ([`crate::set_secrets_dir`]), else in `<mail dir>/secrets/`. Every file
//! an earlier build left in `<mail dir>/secrets/` moves there when the shell
//! starts ([`migrate_all`]; also on a read, for a host that did not), and a password kept as a plain file by an earlier build is moved
//! into the store the first time it is read.
use std::path::{Path, PathBuf};

/// Where one service's passwords go: the host's secrets folder for Mail
/// (`<home>/secrets/os.mail/`, ADR 0004 §11, 0700) when the host names one,
/// else `<mail dir>/secrets/` (a Card runner without a host, tests). The
/// mail dir still names the keychain items, so they stay where they were.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Place {
    /// The service's state folder (`<host dir>/mail`).
    pub mail_dir: PathBuf,
    /// The folder the password files go in.
    pub secrets_dir: PathBuf,
}

impl Place {
    /// Where earlier builds kept the files: `<mail dir>/secrets/`.
    pub fn legacy(mail_dir: &Path) -> Self {
        Place { mail_dir: mail_dir.to_path_buf(), secrets_dir: mail_dir.join("secrets") }
    }

    /// The host's folder when it named one, else [`Place::legacy`].
    pub fn resolve(mail_dir: &Path, secrets: Option<&Path>) -> Self {
        match secrets {
            Some(dir) => Place { mail_dir: mail_dir.to_path_buf(), secrets_dir: dir.to_path_buf() },
            None => Self::legacy(mail_dir),
        }
    }

    fn file(&self, id: &str) -> PathBuf {
        self.secrets_dir.join(id)
    }

    fn legacy_file(&self, id: &str) -> PathBuf {
        self.mail_dir.join("secrets").join(id)
    }

    /// A password an earlier build left under `<mail dir>/secrets/` moves
    /// (bytes as they are: plain, or Android's sealed form) into the
    /// secrets folder, owner-only, the first time it is needed.
    fn adopt_legacy(&self, id: &str) -> bool {
        let (old, new) = (self.legacy_file(id), self.file(id));
        if old == new || !std::fs::symlink_metadata(&old).is_ok_and(|m| m.is_file()) {
            return false;
        }
        // Moved before, but the old copy could not be deleted then: the new
        // one is the password (written by this build or a later sign-in).
        if !new.exists() {
            let Ok(bytes) = std::fs::read(&old) else { return false };
            if write_private(&new, &bytes).is_err() {
                return false;
            }
        }
        let gone = std::fs::remove_file(&old).is_ok();
        let _ = std::fs::remove_dir(self.mail_dir.join("secrets"));
        gone
    }

    /// Both the file and any legacy one.
    fn remove_files(&self, id: &str) {
        let _ = std::fs::remove_file(self.file(id));
        let _ = std::fs::remove_file(self.legacy_file(id));
    }
}

/// Move every password an earlier build left in `<mail dir>/secrets/` into
/// the secrets folder (the shell, at startup), and drop an old copy whose
/// earlier move could not delete it. How many old files went.
pub fn migrate_all(place: &Place) -> usize {
    let Ok(entries) = std::fs::read_dir(place.mail_dir.join("secrets")) else { return 0 };
    let ids: Vec<String> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|id| !id.ends_with(".tmp"))
        .collect();
    ids.iter().filter(|id| place.adopt_legacy(id)).count()
}

pub trait Vault: Send + Sync {
    /// `place` says where the service keeps passwords; `id` is the account.
    fn put(&self, place: &Place, id: &str, secret: &str) -> Result<(), String>;
    fn get(&self, place: &Place, id: &str) -> Result<String, String>;
    fn remove(&self, place: &Place, id: &str);
}

fn missing() -> String {
    "The account's password is missing; sign in again.".into()
}

/// `path` written owner-only (0600) and atomically, in an owner-only (0700)
/// folder.
fn write_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Cannot store the password: {e}"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700)).map_err(|e| format!("Cannot store the password: {e}"))?;
        }
    }
    let temp = path.with_extension("tmp");
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&temp)
            .map_err(|e| format!("Cannot store the password: {e}"))?;
        file.write_all(bytes).map_err(|e| format!("Cannot store the password: {e}"))?;
    }
    #[cfg(not(unix))]
    std::fs::write(&temp, bytes).map_err(|e| format!("Cannot store the password: {e}"))?;
    std::fs::rename(&temp, path).map_err(|e| format!("Cannot store the password: {e}"))
}

/// An owner-only file per account.
pub struct FileVault;

impl Vault for FileVault {
    fn put(&self, place: &Place, id: &str, secret: &str) -> Result<(), String> {
        write_private(&place.file(id), secret.as_bytes())
    }
    fn get(&self, place: &Place, id: &str) -> Result<String, String> {
        place.adopt_legacy(id);
        std::fs::read_to_string(place.file(id)).map_err(|_| missing())
    }
    fn remove(&self, place: &Place, id: &str) {
        place.remove_files(id);
    }
}

/// A plain file left by an earlier build, if any: read, and moved into
/// `vault` on success.
fn migrate(vault: &dyn Vault, place: &Place, id: &str) -> Option<String> {
    place.adopt_legacy(id);
    let secret = std::fs::read_to_string(place.file(id)).ok()?;
    if vault.put(place, id, &secret).is_ok() {
        // Android's vault rewrites the same file; elsewhere it goes.
        if std::fs::read_to_string(place.file(id)).ok().as_deref() == Some(secret.as_str()) {
            let _ = std::fs::remove_file(place.file(id));
        }
    }
    Some(secret)
}

/// The store for this platform. `OCTOSENSE_MAIL_VAULT=file` picks the file
/// store instead: macOS asks the person again for every rebuilt, unsigned
/// development binary that reads a keychain item.
pub fn platform() -> std::sync::Arc<dyn Vault> {
    if std::env::var("OCTOSENSE_MAIL_VAULT").is_ok_and(|v| v == "file") {
        return std::sync::Arc::new(FileVault);
    }
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    return std::sync::Arc::new(keychain::Keychain);
    #[cfg(target_os = "android")]
    return std::sync::Arc::new(android::Keystore);
    #[allow(unreachable_code)]
    std::sync::Arc::new(FileVault)
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
pub mod keychain {
    use super::*;

    /// One keychain item per account. The item's service names the host
    /// directory too, so two profiles on one machine keep separate items.
    pub struct Keychain;

    fn entry(dir: &Path, id: &str) -> Result<keyring::Entry, String> {
        let dir = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
        let profile = crate::network::hash(&dir.to_string_lossy());
        keyring::Entry::new(&format!("OctoSense Mail {}", &profile[..12]), id).map_err(|e| format!("The keychain is unavailable: {e}"))
    }

    impl Vault for Keychain {
        fn put(&self, place: &Place, id: &str, secret: &str) -> Result<(), String> {
            entry(&place.mail_dir, id)?.set_password(secret).map_err(|e| format!("Cannot store the password in the keychain: {e}"))
        }
        fn get(&self, place: &Place, id: &str) -> Result<String, String> {
            match entry(&place.mail_dir, id)?.get_password() {
                Ok(secret) => Ok(secret),
                Err(keyring::Error::NoEntry) => migrate(self, place, id).ok_or_else(missing),
                Err(e) => Err(format!("Cannot read the password from the keychain: {e}")),
            }
        }
        fn remove(&self, place: &Place, id: &str) {
            if let Ok(entry) = entry(&place.mail_dir, id) {
                let _ = entry.delete_credential();
            }
            place.remove_files(id);
        }
    }
}

#[cfg(target_os = "android")]
mod android {
    //! AES-256-GCM with a key generated inside the Android Keystore under
    //! [`ALIAS`]. The key cannot be exported: a copy of the files decrypts
    //! nothing off this device or outside this app.
    use super::*;
    use makepad_jni_sys as jni;
    use std::ffi::CString;
    use std::ptr::null_mut;

    const ALIAS: &str = "octosense.mail.v1";
    /// What an encrypted file starts with; a file without it is an older
    /// plain one.
    const MAGIC: &[u8] = b"OSK1";

    pub struct Keystore;

    impl Vault for Keystore {
        fn put(&self, place: &Place, id: &str, secret: &str) -> Result<(), String> {
            let (iv, sealed) = unsafe { with_env(|env| crypt(env, true, &[], secret.as_bytes())) }?;
            let mut bytes = MAGIC.to_vec();
            bytes.push(iv.len() as u8);
            bytes.extend_from_slice(&iv);
            bytes.extend_from_slice(&sealed);
            write_private(&place.file(id), &bytes)
        }
        fn get(&self, place: &Place, id: &str) -> Result<String, String> {
            place.adopt_legacy(id);
            let bytes = std::fs::read(place.file(id)).map_err(|_| missing())?;
            let Some(rest) = bytes.strip_prefix(MAGIC) else {
                return migrate(self, place, id).ok_or_else(missing);
            };
            let iv_len = *rest.first().ok_or_else(missing)? as usize;
            let iv = rest.get(1..1 + iv_len).ok_or_else(missing)?;
            let sealed = &rest[1 + iv_len..];
            let (_, plain) = unsafe { with_env(|env| crypt(env, false, iv, sealed)) }?;
            String::from_utf8(plain).map_err(|_| missing())
        }
        fn remove(&self, place: &Place, id: &str) {
            place.remove_files(id);
        }
    }

    /// This thread's JNI environment, attached for the call if it was not.
    unsafe fn with_env<T>(f: impl FnOnce(*mut jni::JNIEnv) -> Result<T, String>) -> Result<T, String> {
        let vm = makepad_android_state::get_java_vm();
        if vm.is_null() {
            return Err("The Android keystore is unavailable.".into());
        }
        let mut env: *mut std::ffi::c_void = null_mut();
        let attached_here = ((**vm).GetEnv.unwrap())(vm, &mut env, jni::JNI_VERSION_1_6) != 0;
        if attached_here {
            let mut fresh: *mut jni::JNIEnv = null_mut();
            if ((**vm).AttachCurrentThread.unwrap())(vm, &mut fresh, null_mut()) != 0 {
                return Err("The Android keystore is unavailable.".into());
            }
            env = fresh as _;
        }
        let env = env as *mut jni::JNIEnv;
        ((**env).PushLocalFrame.unwrap())(env, 64);
        let result = f(env);
        ((**env).PopLocalFrame.unwrap())(env, null_mut());
        if attached_here {
            ((**vm).DetachCurrentThread.unwrap())(vm);
        }
        result
    }

    fn c(s: &str) -> CString {
        CString::new(s).unwrap()
    }

    unsafe fn check(env: *mut jni::JNIEnv, what: &str) -> Result<(), String> {
        if ((**env).ExceptionCheck.unwrap())(env) != 0 {
            ((**env).ExceptionClear.unwrap())(env);
            return Err(format!("The Android keystore failed ({what})."));
        }
        Ok(())
    }

    unsafe fn class(env: *mut jni::JNIEnv, name: &str) -> Result<jni::jclass, String> {
        let class = ((**env).FindClass.unwrap())(env, c(name).as_ptr());
        check(env, name)?;
        Ok(class)
    }

    unsafe fn method(env: *mut jni::JNIEnv, class: jni::jclass, name: &str, sig: &str) -> Result<jni::jmethodID, String> {
        let id = ((**env).GetMethodID.unwrap())(env, class, c(name).as_ptr(), c(sig).as_ptr());
        check(env, name)?;
        Ok(id)
    }

    unsafe fn static_method(env: *mut jni::JNIEnv, class: jni::jclass, name: &str, sig: &str) -> Result<jni::jmethodID, String> {
        let id = ((**env).GetStaticMethodID.unwrap())(env, class, c(name).as_ptr(), c(sig).as_ptr());
        check(env, name)?;
        Ok(id)
    }

    unsafe fn string(env: *mut jni::JNIEnv, s: &str) -> jni::jobject {
        ((**env).NewStringUTF.unwrap())(env, c(s).as_ptr())
    }

    unsafe fn string_array(env: *mut jni::JNIEnv, items: &[&str]) -> Result<jni::jobject, String> {
        let string_class = class(env, "java/lang/String")?;
        let array = ((**env).NewObjectArray.unwrap())(env, items.len() as i32, string_class, null_mut());
        for (i, item) in items.iter().enumerate() {
            ((**env).SetObjectArrayElement.unwrap())(env, array, i as i32, string(env, item));
        }
        Ok(array)
    }

    unsafe fn bytes_in(env: *mut jni::JNIEnv, bytes: &[u8]) -> jni::jobject {
        let array = ((**env).NewByteArray.unwrap())(env, bytes.len() as i32);
        ((**env).SetByteArrayRegion.unwrap())(env, array, 0, bytes.len() as i32, bytes.as_ptr() as *const i8);
        array
    }

    unsafe fn bytes_out(env: *mut jni::JNIEnv, array: jni::jobject) -> Vec<u8> {
        if array.is_null() {
            return Vec::new();
        }
        let len = ((**env).GetArrayLength.unwrap())(env, array) as usize;
        let mut out = vec![0u8; len];
        ((**env).GetByteArrayRegion.unwrap())(env, array, 0, len as i32, out.as_mut_ptr() as *mut i8);
        out
    }

    /// The key under [`ALIAS`], made on first use.
    unsafe fn key(env: *mut jni::JNIEnv) -> Result<jni::jobject, String> {
        let keystore_class = class(env, "java/security/KeyStore")?;
        let get_instance = static_method(env, keystore_class, "getInstance", "(Ljava/lang/String;)Ljava/security/KeyStore;")?;
        let keystore = ((**env).CallStaticObjectMethod.unwrap())(env, keystore_class, get_instance, string(env, "AndroidKeyStore"));
        check(env, "KeyStore.getInstance")?;
        let load = method(env, keystore_class, "load", "(Ljava/security/KeyStore$LoadStoreParameter;)V")?;
        ((**env).CallVoidMethod.unwrap())(env, keystore, load, null_mut::<std::ffi::c_void>());
        check(env, "KeyStore.load")?;
        let get_key = method(env, keystore_class, "getKey", "(Ljava/lang/String;[C)Ljava/security/Key;")?;
        let key = ((**env).CallObjectMethod.unwrap())(env, keystore, get_key, string(env, ALIAS), null_mut::<std::ffi::c_void>());
        check(env, "KeyStore.getKey")?;
        if !key.is_null() {
            return Ok(key);
        }
        let builder_class = class(env, "android/security/keystore/KeyGenParameterSpec$Builder")?;
        let init = method(env, builder_class, "<init>", "(Ljava/lang/String;I)V")?;
        // PURPOSE_ENCRYPT | PURPOSE_DECRYPT
        let builder = ((**env).NewObject.unwrap())(env, builder_class, init, string(env, ALIAS), 3 as jni::jint);
        check(env, "KeyGenParameterSpec.Builder")?;
        let builder_sig = "([Ljava/lang/String;)Landroid/security/keystore/KeyGenParameterSpec$Builder;";
        let set_modes = method(env, builder_class, "setBlockModes", builder_sig)?;
        ((**env).CallObjectMethod.unwrap())(env, builder, set_modes, string_array(env, &["GCM"])?);
        let set_padding = method(env, builder_class, "setEncryptionPaddings", builder_sig)?;
        ((**env).CallObjectMethod.unwrap())(env, builder, set_padding, string_array(env, &["NoPadding"])?);
        let set_size = method(env, builder_class, "setKeySize", "(I)Landroid/security/keystore/KeyGenParameterSpec$Builder;")?;
        ((**env).CallObjectMethod.unwrap())(env, builder, set_size, 256 as jni::jint);
        check(env, "KeyGenParameterSpec settings")?;
        let build = method(env, builder_class, "build", "()Landroid/security/keystore/KeyGenParameterSpec;")?;
        let spec = ((**env).CallObjectMethod.unwrap())(env, builder, build);
        check(env, "KeyGenParameterSpec.build")?;
        let generator_class = class(env, "javax/crypto/KeyGenerator")?;
        let generator_instance =
            static_method(env, generator_class, "getInstance", "(Ljava/lang/String;Ljava/lang/String;)Ljavax/crypto/KeyGenerator;")?;
        let generator =
            ((**env).CallStaticObjectMethod.unwrap())(env, generator_class, generator_instance, string(env, "AES"), string(env, "AndroidKeyStore"));
        check(env, "KeyGenerator.getInstance")?;
        let generator_init = method(env, generator_class, "init", "(Ljava/security/spec/AlgorithmParameterSpec;)V")?;
        ((**env).CallVoidMethod.unwrap())(env, generator, generator_init, spec);
        check(env, "KeyGenerator.init")?;
        let generate = method(env, generator_class, "generateKey", "()Ljavax/crypto/SecretKey;")?;
        let key = ((**env).CallObjectMethod.unwrap())(env, generator, generate);
        check(env, "KeyGenerator.generateKey")?;
        Ok(key)
    }

    /// Seal (`encrypt`) or open `data`; sealing picks a fresh IV and returns it.
    unsafe fn crypt(env: *mut jni::JNIEnv, encrypt: bool, iv: &[u8], data: &[u8]) -> Result<(Vec<u8>, Vec<u8>), String> {
        let key = key(env)?;
        let cipher_class = class(env, "javax/crypto/Cipher")?;
        let get_instance = static_method(env, cipher_class, "getInstance", "(Ljava/lang/String;)Ljavax/crypto/Cipher;")?;
        let cipher = ((**env).CallStaticObjectMethod.unwrap())(env, cipher_class, get_instance, string(env, "AES/GCM/NoPadding"));
        check(env, "Cipher.getInstance")?;
        if encrypt {
            let init = method(env, cipher_class, "init", "(ILjava/security/Key;)V")?;
            ((**env).CallVoidMethod.unwrap())(env, cipher, init, 1 as jni::jint, key);
        } else {
            let spec_class = class(env, "javax/crypto/spec/GCMParameterSpec")?;
            let spec_init = method(env, spec_class, "<init>", "(I[B)V")?;
            let spec = ((**env).NewObject.unwrap())(env, spec_class, spec_init, 128 as jni::jint, bytes_in(env, iv));
            check(env, "GCMParameterSpec")?;
            let init = method(env, cipher_class, "init", "(ILjava/security/Key;Ljava/security/spec/AlgorithmParameterSpec;)V")?;
            ((**env).CallVoidMethod.unwrap())(env, cipher, init, 2 as jni::jint, key, spec);
        }
        check(env, "Cipher.init")?;
        let iv = if encrypt {
            let get_iv = method(env, cipher_class, "getIV", "()[B")?;
            bytes_out(env, ((**env).CallObjectMethod.unwrap())(env, cipher, get_iv))
        } else {
            Vec::new()
        };
        let do_final = method(env, cipher_class, "doFinal", "([B)[B")?;
        let out = ((**env).CallObjectMethod.unwrap())(env, cipher, do_final, bytes_in(env, data));
        check(env, "Cipher.doFinal")?;
        Ok((iv, bytes_out(env, out)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mail-vault-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[cfg(unix)]
    fn mode(path: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    /// ADR 0004 §11: passwords live in the host's secrets folder
    /// (`secrets/os.mail/`), not beside the service's state under `apps/`.
    #[test]
    fn should_keep_passwords_in_the_host_secrets_folder_owner_only_when_the_host_names_one() {
        let dir = scratch("place");
        let place = Place { mail_dir: dir.join("apps/.host/mail"), secrets_dir: dir.join("secrets/os.mail") };
        FileVault.put(&place, "a1", "pw").unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("secrets/os.mail/a1")).unwrap(), "pw");
        assert!(!dir.join("apps/.host/mail/secrets").exists(), "nothing under apps/");
        #[cfg(unix)]
        {
            assert_eq!(mode(&dir.join("secrets/os.mail")), 0o700);
            assert_eq!(mode(&dir.join("secrets/os.mail/a1")), 0o600);
        }
        assert_eq!(FileVault.get(&place, "a1").unwrap(), "pw");
        FileVault.remove(&place, "a1");
        assert!(FileVault.get(&place, "a1").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn should_move_a_password_from_the_old_mail_folder_when_it_is_first_read() {
        let dir = scratch("migrate");
        let place = Place { mail_dir: dir.join("apps/.host/mail"), secrets_dir: dir.join("secrets/os.mail") };
        let old = Place::legacy(&place.mail_dir);
        FileVault.put(&old, "a1", "from before").unwrap();
        assert!(dir.join("apps/.host/mail/secrets/a1").is_file());
        assert_eq!(FileVault.get(&place, "a1").unwrap(), "from before");
        assert!(!dir.join("apps/.host/mail/secrets/a1").exists(), "moved out of apps/");
        assert_eq!(std::fs::read_to_string(dir.join("secrets/os.mail/a1")).unwrap(), "from before");
        #[cfg(unix)]
        assert_eq!(mode(&dir.join("secrets/os.mail/a1")), 0o600);
        // Removing an account removes a password wherever it is.
        FileVault.put(&old, "a2", "x").unwrap();
        FileVault.remove(&place, "a2");
        assert!(!dir.join("apps/.host/mail/secrets/a2").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// At startup every password left under the mail folder moves, not only
    /// those read later; a leftover whose move already happened (its delete
    /// failed) is removed then.
    #[test]
    fn should_move_every_old_password_at_startup_and_retry_a_leftover() {
        let dir = scratch("startup");
        let place = Place { mail_dir: dir.join("apps/.host/mail"), secrets_dir: dir.join("secrets/os.mail") };
        let old = Place::legacy(&place.mail_dir);
        FileVault.put(&old, "a1", "one").unwrap();
        FileVault.put(&old, "b2", "two").unwrap();
        // b2 was moved before, but its old copy could not be deleted.
        FileVault.put(&place, "b2", "two").unwrap();
        assert_eq!(migrate_all(&place), 2);
        assert!(!dir.join("apps/.host/mail/secrets").exists(), "nothing left under apps/");
        assert_eq!(std::fs::read_to_string(dir.join("secrets/os.mail/a1")).unwrap(), "one");
        assert_eq!(std::fs::read_to_string(dir.join("secrets/os.mail/b2")).unwrap(), "two");
        assert_eq!(migrate_all(&place), 0, "nothing to do the second time");
        // Without a host folder (the legacy place itself) nothing moves.
        assert_eq!(migrate_all(&old), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn should_use_the_old_mail_folder_when_no_host_secrets_folder_is_set() {
        let mail = Path::new("/h/apps/.host/mail");
        assert_eq!(Place::resolve(mail, None).secrets_dir, mail.join("secrets"));
        assert_eq!(Place::resolve(mail, Some(Path::new("/h/secrets/os.mail"))).secrets_dir, Path::new("/h/secrets/os.mail"));
    }

    #[test]
    fn a_file_vault_keeps_secrets_owner_only() {
        let dir = Place::legacy(&std::env::temp_dir().join(format!("mail-vault-{}", std::process::id())));
        FileVault.put(&dir, "a1", "pa ss").unwrap();
        assert_eq!(FileVault.get(&dir, "a1").unwrap(), "pa ss");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.file("a1")).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        FileVault.remove(&dir, "a1");
        assert!(FileVault.get(&dir, "a1").is_err());
        let _ = std::fs::remove_dir_all(&dir.mail_dir);
    }

    /// Touches the login keychain, so it runs only when asked:
    /// `cargo test -- --ignored keychain`.
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore]
    fn the_keychain_keeps_a_secret_and_takes_over_an_old_file() {
        let dir = Place::legacy(&std::env::temp_dir().join(format!("mail-keychain-{}", std::process::id())));
        FileVault.put(&dir, "old", "from a file").unwrap();
        let vault = keychain::Keychain;
        assert_eq!(vault.get(&dir, "old").unwrap(), "from a file");
        assert!(!dir.file("old").exists(), "the file moved into the keychain");
        assert_eq!(vault.get(&dir, "old").unwrap(), "from a file");
        vault.put(&dir, "new", "s3cret").unwrap();
        assert_eq!(vault.get(&dir, "new").unwrap(), "s3cret");
        vault.remove(&dir, "old");
        vault.remove(&dir, "new");
        assert!(vault.get(&dir, "new").is_err());
        let _ = std::fs::remove_dir_all(&dir.mail_dir);
    }
}
