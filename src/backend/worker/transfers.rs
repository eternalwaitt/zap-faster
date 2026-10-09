//! Bounded streamed attachment transfers using the protocol library's verifier.
use super::*;
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use whatsapp_rust::wacore::upload::UploadSource;

tokio::task_local! { static UPLOAD: Control; }

pub(super) fn upload_token() -> Option<u64> {
    UPLOAD.try_with(|control| control.token).ok()
}

pub(super) struct PendingUpload {
    control: Control,
    operation: std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>,
}

impl Worker {
    pub(super) fn spawn_upload(
        &mut self,
        chat: ChatId,
        operation: impl std::future::Future<Output = ()> + Send + 'static,
    ) {
        self.transfer_sequence += 1;
        let token = self.transfer_sequence;
        let control = Control::new(
            token,
            self.commands.clone(),
            chat.clone(),
            String::new(),
            None,
            None,
            0,
        );
        let order = match self.reserve_send_order() {
            Ok(order) => order,
            Err(error) => {
                log::warn!("could not reserve upload order: {error}");
                self.emit(Event::Error(
                    "Could not prepare the attachment. Check local storage.".into(),
                ));
                return;
            }
        };
        self.upload_order.insert(token, order);
        self.uploads.insert(token, control.clone());
        self.emit(Event::UploadProgress {
            token,
            chat,
            bytes: 0,
            total: 0,
        });
        self.upload_pending.push_back(PendingUpload {
            control,
            operation: Box::pin(operation),
        });
        self.pump_uploads();
    }

    pub(super) fn pump_uploads(&mut self) {
        if self.upload_running.is_some() {
            return;
        }
        let Some(PendingUpload { control, operation }) = self.upload_pending.pop_front() else {
            return;
        };
        let token = control.token;
        self.upload_running = Some(token);
        let commands = self.commands.clone();
        tokio::spawn(async move {
            let result = control
                .run(Duration::from_secs(30 * 60), async {
                    UPLOAD.scope(control.clone(), operation).await;
                    Ok(())
                })
                .await;
            let _ = commands.send(Command::UploadFinished {
                token,
                failed: result.is_err() && !control.cancelled(),
            });
        });
    }
}

struct UploadBytes {
    bytes: Arc<[u8]>,
    control: Control,
}
struct UploadReader {
    reader: io::Cursor<Arc<[u8]>>,
    control: Control,
    notified: Instant,
}
impl Read for UploadReader {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        self.control.check()?;
        let count = self.reader.read(output)?;
        if count == 0 || self.notified.elapsed() >= Duration::from_millis(150) {
            self.control.upload_progress(
                self.reader.position(),
                self.reader.get_ref().as_ref().len() as u64,
            );
            self.notified = Instant::now();
        }
        Ok(count)
    }
}
impl UploadSource for UploadBytes {
    fn len(&self) -> u64 {
        self.bytes.as_ref().len() as u64
    }
    fn reader_from(&self, offset: u64) -> io::Result<Box<dyn Read + Send>> {
        self.control.check()?;
        let mut reader = io::Cursor::new(self.bytes.clone());
        reader.set_position(offset.min(self.len()));
        Ok(Box::new(UploadReader {
            reader,
            control: self.control.clone(),
            notified: Instant::now() - Duration::from_secs(1),
        }))
    }
}

pub(super) async fn upload(
    client: &Client,
    bytes: Vec<u8>,
    kind: MediaType,
) -> Result<whatsapp_rust::upload::UploadResponse, String> {
    let Ok(control) = UPLOAD.try_with(Clone::clone) else {
        return client
            .upload(bytes, kind, UploadOptions::default())
            .await
            .map_err(|error| error.to_string());
    };
    control.check().map_err(|error| error.to_string())?;
    let (encrypted, info) = tokio::task::spawn_blocking(move || {
        let mut encrypted =
            Vec::with_capacity(whatsapp_rust::wacore::upload::encrypted_len(bytes.len()));
        let info = whatsapp_rust::wacore::upload::encrypt_media_streaming(
            &bytes[..],
            &mut encrypted,
            kind,
        )
        .map_err(|error| error.to_string())?;
        Ok::<_, String>((encrypted, info))
    })
    .await
    .map_err(|error| error.to_string())??;
    control.upload_progress(0, encrypted.len() as u64);
    client
        .upload_stream(
            UploadBytes {
                bytes: encrypted.into(),
                control,
            },
            info,
            kind,
        )
        .await
        .map_err(|error| error.to_string())
}

#[derive(Clone)]
pub(super) struct Control {
    pub token: u64,
    cancelled: Arc<AtomicBool>,
    wake: Arc<tokio::sync::Notify>,
    commands: mpsc::UnboundedSender<Command>,
    chat: ChatId,
    message: String,
    card: Option<usize>,
    total: Option<u64>,
    limit: u64,
}

impl Control {
    fn upload_progress(&self, bytes: u64, total: u64) {
        let _ = self.commands.send(Command::UploadProgress {
            token: self.token,
            chat: self.chat.clone(),
            bytes,
            total,
        });
    }
    pub fn new(
        token: u64,
        commands: mpsc::UnboundedSender<Command>,
        chat: ChatId,
        message: String,
        card: Option<usize>,
        total: Option<u64>,
        limit: u64,
    ) -> Self {
        Self {
            token,
            commands,
            chat,
            message,
            card,
            total,
            limit,
            cancelled: Arc::new(AtomicBool::new(false)),
            wake: Arc::new(tokio::sync::Notify::new()),
        }
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        self.wake.notify_one();
    }
    pub fn cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
    pub fn check(&self) -> io::Result<()> {
        if self.cancelled() {
            Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "Download cancelled",
            ))
        } else {
            Ok(())
        }
    }
    pub async fn run<T>(
        &self,
        deadline: Duration,
        operation: impl std::future::Future<Output = Result<T, String>>,
    ) -> Result<T, String> {
        self.check().map_err(|e| e.to_string())?;
        tokio::select! {
            biased;
            _ = self.wake.notified() => Err("Download cancelled".into()),
            result = with_attachment_deadline(deadline, operation) => result,
        }
    }
    pub fn progress(&self, bytes: u64) {
        let _ = self.commands.send(Command::TransferProgress {
            chat: self.chat.clone(),
            message: self.message.clone(),
            card: self.card,
            token: self.token,
            bytes,
            total: self.total,
        });
    }
}

/// Owns staging cleanup even when timeout drops the future while the library's
/// blocking writer is still running. Close before unlinking on Windows.
struct StagingWriter {
    file: Option<std::fs::File>,
    path: PathBuf,
    control: Control,
    notified: Instant,
}

impl Drop for StagingWriter {
    fn drop(&mut self) {
        self.file.take();
        let _ = std::fs::remove_file(&self.path);
    }
}

impl Write for StagingWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.control.check()?;
        let file = self.file.as_mut().expect("open staging file");
        let position = file.stream_position()?;
        if bytes.len() as u64 > self.control.limit.saturating_sub(position) {
            return Err(io::Error::other(
                "This attachment exceeds the download limit",
            ));
        }
        let written = file.write(bytes)?;
        if self.notified.elapsed() >= Duration::from_millis(150) {
            self.control.progress(position + written as u64);
            self.notified = Instant::now();
        }
        Ok(written)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.file.as_mut().expect("open staging file").flush()
    }
}
impl Seek for StagingWriter {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.file
            .as_mut()
            .expect("open staging file")
            .seek(position)
    }
}
impl DownloadWriter for StagingWriter {
    fn truncate(&mut self, len: u64) -> io::Result<()> {
        self.file.as_mut().expect("open staging file").set_len(len)
    }
}

pub(super) async fn download(
    client: &Client,
    media: &dyn Downloadable,
    dir: &Path,
    path: &Path,
    control: Control,
) -> Result<PathBuf, String> {
    control.check().map_err(|e| e.to_string())?;
    tokio::fs::create_dir_all(dir)
        .await
        .map_err(|e| e.to_string())?;
    let (temporary, file) = temporary_attachment_file(path)?;
    let writer = StagingWriter {
        file: Some(file),
        path: temporary.clone(),
        control: control.clone(),
        notified: Instant::now(),
    };
    // This returns only after the library checks ciphertext hash, MAC and
    // plaintext hash. No partial file is published or exposed to the UI.
    let mut writer = client
        .download_to_writer(media, writer)
        .await
        .map_err(|error| {
            if control.cancelled() {
                "Download cancelled".to_owned()
            } else {
                error.to_string()
            }
        })?;
    control.check().map_err(|e| e.to_string())?;
    let file = writer.file.take().expect("verified staging file");
    let size = file.metadata().map_err(|e| e.to_string())?.len();
    if control.total.is_some_and(|expected| expected != size) {
        return Err("Attachment size verification failed".into());
    }
    file.sync_all().map_err(|e| e.to_string())?;
    drop(file);
    control.check().map_err(|e| e.to_string())?;
    publish_attachment(&temporary, path).await?;
    control.progress(size);
    Ok(path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn writer(
        limit: u64,
    ) -> (
        tempfile::TempDir,
        StagingWriter,
        mpsc::UnboundedReceiver<Command>,
    ) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("fixture.part");
        let (commands, receiver) = mpsc::unbounded_channel();
        let control = Control::new(
            42,
            commands,
            "synthetic".into(),
            "attachment".into(),
            None,
            Some(limit),
            limit,
        );
        let writer = StagingWriter {
            file: Some(std::fs::File::create(&path).unwrap()),
            path,
            control,
            notified: Instant::now() - Duration::from_secs(1),
        };
        (directory, writer, receiver)
    }

    #[test]
    fn bounded_writer_reports_actual_bytes_and_cancellation_removes_staging() {
        let (_directory, mut writer, mut receiver) = writer(8);
        writer.write_all(&[1; 4]).unwrap();
        assert!(matches!(
            receiver.try_recv(),
            Ok(Command::TransferProgress {
                token: 42,
                bytes: 4,
                total: Some(8),
                ..
            })
        ));
        assert!(writer.write_all(&[2; 5]).is_err());
        assert_eq!(writer.file.as_ref().unwrap().metadata().unwrap().len(), 4);
        writer.control.cancel();
        assert_eq!(
            writer.write(&[3]).unwrap_err().kind(),
            io::ErrorKind::Interrupted
        );
        let path = writer.path.clone();
        drop(writer);
        assert!(!path.exists());
    }

    #[test]
    fn larger_manual_policy_does_not_change_automatic_policy() {
        let size = 200 * 1024 * 1024;
        assert!(attachment_is_too_large(Some(size)));
        assert!(size < crate::model::MANUAL_DOWNLOAD_LIMIT);
        let (_directory, mut writer, _) = writer(size);
        writer
            .seek(SeekFrom::Start(ATTACHMENT_DOWNLOAD_LIMIT))
            .unwrap();
        writer.write_all(&[1; 4]).unwrap();
        assert_eq!(
            writer.file.as_ref().unwrap().metadata().unwrap().len(),
            ATTACHMENT_DOWNLOAD_LIMIT + 4
        );
    }
}
