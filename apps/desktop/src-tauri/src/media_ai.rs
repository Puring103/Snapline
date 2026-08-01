use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use reqwest::Client;
use sha2::{Digest, Sha256};
use snapline_crypto::MasterKey;
use snapline_desktop_core::Repository;
use uuid::Uuid;

use crate::ai::AiAttachment;

const FFMPEG_URL: &str =
    "https://www.gyan.dev/ffmpeg/builds/packages/ffmpeg-8.1.2-essentials_build.zip";
const FFMPEG_SHA256: &str = "db580001caa24ac104c8cb856cd113a87b0a443f7bdf47d8c12b1d740584a2ec";
const MAX_FFMPEG_DOWNLOAD_BYTES: u64 = 130 * 1024 * 1024;

pub async fn ensure_ffmpeg(data_dir: &Path, client: &Client) -> Result<PathBuf, String> {
    let tools = data_dir.join("tools").join("ffmpeg-8.1.2");
    let binary = tools.join("ffmpeg.exe");
    if binary.is_file() {
        return Ok(binary);
    }
    fs::create_dir_all(&tools).map_err(|_| "无法创建本地媒体工具目录".to_string())?;
    let archive = tools.join("ffmpeg.zip.partial");
    let response = client
        .get(FFMPEG_URL)
        .send()
        .await
        .map_err(|_| "无法下载视频处理组件".to_string())?;
    if !response.status().is_success()
        || response
            .content_length()
            .is_some_and(|size| size > MAX_FFMPEG_DOWNLOAD_BYTES)
    {
        return Err("视频处理组件下载响应无效".to_string());
    }
    let mut response = response;
    let mut output = File::create(&archive).map_err(|_| "无法写入视频处理组件".to_string())?;
    let mut hasher = Sha256::new();
    let mut downloaded = 0_u64;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "视频处理组件下载中断".to_string())?
    {
        downloaded = downloaded
            .checked_add(chunk.len() as u64)
            .filter(|size| *size <= MAX_FFMPEG_DOWNLOAD_BYTES)
            .ok_or_else(|| "视频处理组件超过大小上限".to_string())?;
        std::io::Write::write_all(&mut output, &chunk)
            .map_err(|_| "无法写入视频处理组件".to_string())?;
        hasher.update(&chunk);
    }
    std::io::Write::flush(&mut output).map_err(|_| "无法完成视频处理组件写入".to_string())?;
    drop(output);
    if format!("{:x}", hasher.finalize()) != FFMPEG_SHA256 {
        let _ = fs::remove_file(&archive);
        return Err("视频处理组件完整性校验失败".to_string());
    }
    let extracted = tools.join("extracted");
    fs::create_dir_all(&extracted).map_err(|_| "无法创建视频处理组件解包目录".to_string())?;
    let status = Command::new("tar")
        .args(["-xf"])
        .arg(&archive)
        .args(["-C"])
        .arg(&extracted)
        .status()
        .map_err(|_| "Windows tar 不可用，无法解包视频处理组件".to_string())?;
    if !status.success() {
        return Err("无法解包视频处理组件".to_string());
    }
    let found = find_ffmpeg(&extracted).ok_or_else(|| "视频处理组件缺少 ffmpeg.exe".to_string())?;
    fs::copy(found, &binary).map_err(|_| "无法安装视频处理组件".to_string())?;
    let _ = fs::remove_file(&archive);
    let _ = fs::remove_dir_all(&extracted);
    Ok(binary)
}

fn find_ffmpeg(directory: &Path) -> Option<PathBuf> {
    let entries = fs::read_dir(directory).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file()
            && path
                .file_name()
                .is_some_and(|name| name.eq_ignore_ascii_case("ffmpeg.exe"))
        {
            return Some(path);
        }
        if path.is_dir()
            && let Some(found) = find_ffmpeg(&path)
        {
            return Some(found);
        }
    }
    None
}

pub fn extract_video_inputs(
    repository: &Repository,
    master_key: &MasterKey,
    id: Uuid,
    ffmpeg: &Path,
) -> Result<Vec<AiAttachment>, String> {
    let frames = run_ffmpeg(
        repository,
        master_key,
        id,
        ffmpeg,
        &[
            "-vf",
            "select=isnan(prev_selected_t)+gte(t-prev_selected_t\\,60),scale=1280:-2",
            "-frames:v",
            "30",
            "-an",
            "-f",
            "image2pipe",
            "-vcodec",
            "mjpeg",
            "-pix_fmt",
            "yuvj420p",
            "pipe:1",
        ],
    )?;
    let audio = extract_audio_bytes(repository, master_key, id, ffmpeg)?;
    let mut inputs = split_jpegs(&frames)
        .into_iter()
        .enumerate()
        .map(|(index, bytes)| AiAttachment {
            media_type: "image/jpeg".into(),
            display_name: format!("视频关键帧-{}", index + 1),
            bytes,
        })
        .collect::<Vec<_>>();
    if !audio.is_empty() {
        inputs.push(AiAttachment {
            media_type: "audio/mpeg".into(),
            display_name: "视频音轨".into(),
            bytes: audio,
        });
    }
    if inputs.is_empty() {
        return Err("无法从视频抽取关键帧或音轨".to_string());
    }
    Ok(inputs)
}

pub fn extract_audio_input(
    repository: &Repository,
    master_key: &MasterKey,
    id: Uuid,
    ffmpeg: &Path,
) -> Result<AiAttachment, String> {
    let bytes = extract_audio_bytes(repository, master_key, id, ffmpeg)?;
    if bytes.is_empty() {
        return Err("无法从附件抽取音频".to_string());
    }
    Ok(AiAttachment {
        media_type: "audio/mpeg".into(),
        display_name: "压缩音频".into(),
        bytes,
    })
}

pub fn extract_image_input(
    repository: &Repository,
    master_key: &MasterKey,
    id: Uuid,
    ffmpeg: &Path,
) -> Result<AiAttachment, String> {
    let output = run_ffmpeg(
        repository,
        master_key,
        id,
        ffmpeg,
        &[
            "-vf",
            "scale=1280:-2",
            "-frames:v",
            "1",
            "-an",
            "-f",
            "image2pipe",
            "-vcodec",
            "mjpeg",
            "-pix_fmt",
            "yuvj420p",
            "pipe:1",
        ],
    )?;
    let bytes = split_jpegs(&output)
        .into_iter()
        .next()
        .ok_or_else(|| "无法缩放图片附件".to_string())?;
    Ok(AiAttachment {
        media_type: "image/jpeg".into(),
        display_name: "缩放图片".into(),
        bytes,
    })
}

fn extract_audio_bytes(
    repository: &Repository,
    master_key: &MasterKey,
    id: Uuid,
    ffmpeg: &Path,
) -> Result<Vec<u8>, String> {
    run_ffmpeg(
        repository,
        master_key,
        id,
        ffmpeg,
        &[
            "-vn", "-ac", "1", "-ar", "16000", "-b:a", "32k", "-f", "mp3", "pipe:1",
        ],
    )
}

fn run_ffmpeg(
    repository: &Repository,
    master_key: &MasterKey,
    id: Uuid,
    ffmpeg: &Path,
    output_args: &[&str],
) -> Result<Vec<u8>, String> {
    let mut child = Command::new(ffmpeg)
        .args(["-hide_banner", "-loglevel", "error", "-i", "pipe:0"])
        .args(output_args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| "无法启动视频处理组件".to_string())?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| "无法打开视频处理输入".to_string())?;
    let (write_result, output) = std::thread::scope(|scope| {
        let writer = scope.spawn(move || {
            let mut stdin = stdin;
            repository.read_attachment(master_key, id, &mut stdin)
        });
        let output = child.wait_with_output();
        (writer.join(), output)
    });
    write_result
        .map_err(|_| "视频解密输入线程异常".to_string())?
        .map_err(|_| "无法解密视频附件".to_string())?;
    let output = output.map_err(|_| "无法读取视频处理结果".to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr)
            .chars()
            .take(500)
            .collect());
    }
    Ok(output.stdout)
}

fn split_jpegs(bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut images = Vec::new();
    let mut start = None;
    let mut index = 0;
    while index + 1 < bytes.len() {
        if bytes[index..index + 2] == [0xff, 0xd8] && start.is_none() {
            start = Some(index);
        } else if bytes[index..index + 2] == [0xff, 0xd9]
            && let Some(from) = start.take()
        {
            images.push(bytes[from..index + 2].to_vec());
        }
        index += 1;
    }
    images
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_concatenated_jpeg_frames_without_temp_files() {
        let bytes = [
            b"noise".as_slice(),
            &[0xff, 0xd8, 1, 2, 0xff, 0xd9],
            &[0xff, 0xd8, 3, 0xff, 0xd9],
        ]
        .concat();
        assert_eq!(
            split_jpegs(&bytes),
            vec![
                vec![0xff, 0xd8, 1, 2, 0xff, 0xd9],
                vec![0xff, 0xd8, 3, 0xff, 0xd9]
            ]
        );
    }

    #[test]
    fn pinned_ffmpeg_download_has_https_and_sha256() {
        assert!(FFMPEG_URL.starts_with("https://"));
        assert_eq!(FFMPEG_SHA256.len(), 64);
    }

    #[tokio::test]
    #[ignore = "downloads the pinned FFmpeg build; requires SNAPLINE_MEDIA_AI_TEST=1"]
    async fn pinned_ffmpeg_extracts_encrypted_video_without_plaintext_output() {
        if std::env::var("SNAPLINE_MEDIA_AI_TEST").as_deref() != Ok("1") {
            return;
        }
        let directory = tempfile::tempdir().unwrap();
        let tool_cache =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../target/media-ai-test-tools");
        let client = Client::builder().build().unwrap();
        let ffmpeg = ensure_ffmpeg(&tool_cache, &client).await.unwrap();
        let plaintext = directory.path().join("fixture.mp4");
        let status = Command::new(&ffmpeg)
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=320x180:rate=10",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=880:sample_rate=16000",
                "-t",
                "2",
                "-pix_fmt",
                "yuv420p",
            ])
            .arg(&plaintext)
            .status()
            .unwrap();
        assert!(status.success());
        let repository = Repository::open(directory.path().join("snapline.db")).unwrap();
        let master_key = MasterKey::generate();
        let id = Uuid::new_v4();
        repository
            .save_attachment(&master_key, id, File::open(&plaintext).unwrap())
            .unwrap();
        fs::remove_file(&plaintext).unwrap();
        let inputs = extract_video_inputs(&repository, &master_key, id, &ffmpeg).unwrap();
        assert!(inputs.iter().any(|input| input.media_type == "image/jpeg"));
        assert!(inputs.iter().any(|input| input.media_type == "audio/mpeg"));
        assert!(!plaintext.exists());
    }
}
