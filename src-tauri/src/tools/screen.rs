//! # Visão de Tela (DXGI / PrintWindow), Automação Win32 e Clipboard — `tools/screen.rs`
//!
//! Captura desktop e janelas em segundo plano com proteção concorrente via
//! ScreenCaptureGuard (WDA_EXCLUDEFROMCAPTURE + Mutex RAII), realiza clique,
//! digitação e atalhos nativos (SendInput), além de ler texto e imagens DIB
//! da área de transferência do Windows.

use std::sync::OnceLock;
use std::time::Duration;
use tauri::AppHandle;
#[cfg(windows)]
pub(crate) fn encode_bgra_to_jpeg_base64(
    bgra: &[u8],
    real_w: u32,
    real_h: u32,
    row_pitch: usize,
) -> Result<(u32, u32, u32, u32, String), String> {
    use base64::Engine as _;
    use image::codecs::jpeg::JpegEncoder;
    use image::imageops::{resize, FilterType};
    use image::RgbImage;

    let w = real_w as usize;
    let h = real_h as usize;
    let mut rgb = Vec::with_capacity(w * h * 3);
    for y in 0..h {
        let row_start = y * row_pitch;
        let row = &bgra[row_start..row_start + w * 4];
        for px in row.as_chunks::<4>().0 {
            rgb.push(px[2]); // R
            rgb.push(px[1]); // G
            rgb.push(px[0]); // B
        }
    }

    let full_img = RgbImage::from_raw(real_w, real_h, rgb)
        .ok_or_else(|| "Buffer de imagem inválido.".to_string())?;

    let max_w = 1280u32;
    let (out_img, cap_w, cap_h) = if real_w > max_w {
        let scale = max_w as f64 / real_w as f64;
        let tw = max_w;
        let th = ((real_h as f64 * scale).round() as u32).max(1);
        (resize(&full_img, tw, th, FilterType::Triangle), tw, th)
    } else {
        (full_img, real_w, real_h)
    };

    let mut jpeg_bytes = Vec::with_capacity(96 * 1024);
    let mut enc = JpegEncoder::new_with_quality(&mut jpeg_bytes, 78);
    enc.encode_image(&out_img)
        .map_err(|e| format!("Falha ao codificar JPEG da tela: {e}"))?;

    let b64 = base64::engine::general_purpose::STANDARD.encode(&jpeg_bytes);
    Ok((cap_w, cap_h, real_w, real_h, b64))
}

#[cfg(windows)]
fn capture_via_dxgi() -> Result<(u32, u32, u32, u32, String), String> {
    use windows::core::Interface;
    use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
    use windows::Win32::Graphics::Direct3D11::{
        D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D,
        D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAPPED_SUBRESOURCE,
        D3D11_MAP_READ, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
    };
    use windows::Win32::Graphics::Dxgi::{
        IDXGIDevice, IDXGIOutput1, IDXGIResource, DXGI_OUTDUPL_FRAME_INFO,
    };

    unsafe {
        let mut device_opt: Option<ID3D11Device> = None;
        let mut ctx_opt: Option<ID3D11DeviceContext> = None;
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            windows::Win32::Foundation::HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut device_opt),
            None,
            Some(&mut ctx_opt),
        )
        .map_err(|e| format!("D3D11CreateDevice falhou: {e}"))?;

        let device = device_opt.ok_or("Dispositivo D3D11 nulo")?;
        let ctx = ctx_opt.ok_or("Contexto D3D11 nulo")?;
        let dxgi_dev: IDXGIDevice = device.cast().map_err(|e| format!("IDXGIDevice: {e}"))?;
        let adapter = dxgi_dev
            .GetAdapter()
            .map_err(|e| format!("GetAdapter: {e}"))?;
        let output = adapter
            .EnumOutputs(0)
            .map_err(|e| format!("EnumOutputs: {e}"))?;
        let output1: IDXGIOutput1 = output.cast().map_err(|e| format!("IDXGIOutput1: {e}"))?;
        let dupl = output1
            .DuplicateOutput(&device)
            .map_err(|e| format!("DuplicateOutput: {e}"))?;

        let mut frame_info = DXGI_OUTDUPL_FRAME_INFO::default();
        let mut res_opt: Option<IDXGIResource> = None;
        let mut acquired = false;
        for _ in 0..8 {
            match dupl.AcquireNextFrame(250, &mut frame_info, &mut res_opt) {
                Ok(()) => {
                    if frame_info.LastPresentTime != 0 || res_opt.is_some() {
                        acquired = true;
                        break;
                    }
                    let _ = dupl.ReleaseFrame();
                }
                Err(_) => {
                    std::thread::sleep(Duration::from_millis(60));
                }
            }
        }
        if !acquired {
            return Err("DXGI não recebeu novo frame da tela.".to_string());
        }

        let res = res_opt.ok_or("DXGI retornou recurso vazio")?;
        let desktop_tex: ID3D11Texture2D =
            res.cast().map_err(|e| format!("ID3D11Texture2D: {e}"))?;
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        desktop_tex.GetDesc(&mut desc);

        let staging_desc = D3D11_TEXTURE2D_DESC {
            Width: desc.Width,
            Height: desc.Height,
            MipLevels: 1,
            ArraySize: 1,
            Format: desc.Format,
            SampleDesc: desc.SampleDesc,
            Usage: D3D11_USAGE_STAGING,
            BindFlags: 0,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            MiscFlags: 0,
        };

        let mut staging_opt: Option<ID3D11Texture2D> = None;
        device
            .CreateTexture2D(&staging_desc, None, Some(&mut staging_opt))
            .map_err(|e| {
                let _ = dupl.ReleaseFrame();
                format!("CreateTexture2D staging: {e}")
            })?;
        let staging = staging_opt.ok_or_else(|| {
            let _ = dupl.ReleaseFrame();
            "Staging texture nula".to_string()
        })?;

        ctx.CopyResource(&staging, &desktop_tex);
        let _ = dupl.ReleaseFrame();

        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        ctx.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
            .map_err(|e| format!("Map staging: {e}"))?;

        let row_pitch = mapped.RowPitch as usize;
        let total = row_pitch * (desc.Height as usize);
        let slice = std::slice::from_raw_parts(mapped.pData as *const u8, total);
        let copied = slice.to_vec();
        ctx.Unmap(&staging, 0);

        encode_bgra_to_jpeg_base64(&copied, desc.Width, desc.Height, row_pitch)
    }
}

#[cfg(windows)]
fn capture_via_gdi() -> Result<(u32, u32, u32, u32, String), String> {
    use std::ffi::c_void;
    use windows::core::w;
    use windows::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateDCW, DeleteDC, DeleteObject,
        GetDC, GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
        CAPTUREBLT, DIB_RGB_COLORS, SRCCOPY,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};

    unsafe {
        let real_w = GetSystemMetrics(SM_CXSCREEN);
        let real_h = GetSystemMetrics(SM_CYSCREEN);
        if real_w <= 0 || real_h <= 0 {
            return Err("Resolução de tela indisponível.".to_string());
        }

        let mut used_create_dc = false;
        let mut screen_dc = CreateDCW(w!("DISPLAY"), None, None, None);
        if !screen_dc.is_invalid() {
            used_create_dc = true;
        } else {
            screen_dc = GetDC(None);
        }
        if screen_dc.is_invalid() {
            return Err("Falha ao obter contexto gráfico (DC) da tela.".to_string());
        }

        let release_screen_dc = |dc| {
            if used_create_dc {
                let _ = DeleteDC(dc);
            } else {
                let _ = ReleaseDC(None, dc);
            }
        };

        let mem_dc = CreateCompatibleDC(Some(screen_dc));
        if mem_dc.is_invalid() {
            release_screen_dc(screen_dc);
            return Err("Falha ao criar contexto gráfico em memória.".to_string());
        }

        let hbmp = CreateCompatibleBitmap(screen_dc, real_w, real_h);
        if hbmp.is_invalid() {
            let _ = DeleteDC(mem_dc);
            release_screen_dc(screen_dc);
            return Err("Falha ao alocar bitmap da tela.".to_string());
        }

        let prev = SelectObject(mem_dc, hbmp.into());
        if prev.is_invalid() {
            let _ = DeleteObject(hbmp.into());
            let _ = DeleteDC(mem_dc);
            release_screen_dc(screen_dc);
            return Err("SelectObject falhou ao associar bitmap.".to_string());
        }

        let mut blt_ok = BitBlt(
            mem_dc,
            0,
            0,
            real_w,
            real_h,
            Some(screen_dc),
            0,
            0,
            SRCCOPY | CAPTUREBLT,
        );
        if blt_ok.is_err() {
            blt_ok = BitBlt(mem_dc, 0, 0, real_w, real_h, Some(screen_dc), 0, 0, SRCCOPY);
        }

        let _ = SelectObject(mem_dc, prev);

        let mut bgra = vec![0u8; (real_w as usize) * (real_h as usize) * 4];
        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: real_w,
                biHeight: -real_h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let scanlines = if blt_ok.is_ok() {
            GetDIBits(
                screen_dc,
                hbmp,
                0,
                real_h as u32,
                Some(bgra.as_mut_ptr() as *mut c_void),
                &mut bmi,
                DIB_RGB_COLORS,
            )
        } else {
            0
        };

        let _ = DeleteObject(hbmp.into());
        let _ = DeleteDC(mem_dc);
        release_screen_dc(screen_dc);

        if let Err(err) = blt_ok {
            return Err(format!("Falha no BitBlt da tela via GDI: {err}"));
        }
        if scanlines == 0 {
            return Err("Falha no GetDIBits ao extrair pixels da tela.".to_string());
        }

        encode_bgra_to_jpeg_base64(&bgra, real_w as u32, real_h as u32, (real_w as usize) * 4)
    }
}

pub fn capture_desktop_screen_blocking() -> Result<(u32, u32, u32, u32, String), String> {
    #[cfg(windows)]
    {
        match capture_via_dxgi() {
            Ok(shot) => Ok(shot),
            Err(dxgi_err) => {
                capture_via_gdi().map_err(|gdi_err| format!("DXGI ({dxgi_err}) · GDI ({gdi_err})"))
            }
        }
    }
    #[cfg(not(windows))]
    {
        Err("Captura de tela disponível apenas no Windows.".to_string())
    }
}

static SCREEN_CAPTURE_MUTEX: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

pub(crate) struct ScreenCaptureGuard {
    app: AppHandle,
    _lock: tokio::sync::MutexGuard<'static, ()>,
}

impl Drop for ScreenCaptureGuard {
    fn drop(&mut self) {
        let _ = set_jarvis_capture_exclusion(&self.app, false);
    }
}

pub(crate) async fn acquire_screen_capture_guard(app: &AppHandle) -> ScreenCaptureGuard {
    let mtx = SCREEN_CAPTURE_MUTEX.get_or_init(|| tokio::sync::Mutex::new(()));
    let lock = mtx.lock().await;
    let excluded = set_jarvis_capture_exclusion(app, true);
    if !excluded.is_empty() {
        tokio::time::sleep(Duration::from_millis(85)).await;
    }
    ScreenCaptureGuard {
        app: app.clone(),
        _lock: lock,
    }
}

pub(crate) fn set_jarvis_capture_exclusion(app: &AppHandle, exclude: bool) -> Vec<isize> {
    #[cfg(windows)]
    {
        use tauri::Manager;
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{
            SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE, WDA_NONE,
        };

        let affinity = if exclude {
            WDA_EXCLUDEFROMCAPTURE
        } else {
            WDA_NONE
        };
        let mut applied = Vec::new();
        for label in ["main", "mascot"] {
            if let Some(win) = app.get_webview_window(label) {
                if let Ok(hwnd_raw) = win.hwnd() {
                    let hwnd = HWND(hwnd_raw.0);
                    if unsafe { SetWindowDisplayAffinity(hwnd, affinity) }.is_ok() {
                        applied.push(hwnd_raw.0 as isize);
                    }
                }
            }
        }
        applied
    }
    #[cfg(not(windows))]
    {
        let _ = (app, exclude);
        Vec::new()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct OpenWindowInfo {
    pub(crate) hwnd: isize,
    #[allow(dead_code)]
    pub(crate) pid: u32,
    pub(crate) process_name: String,
    pub(crate) title: String,
    pub(crate) minimized: bool,
    pub(crate) width: i32,
    pub(crate) height: i32,
}

pub(crate) fn list_open_windows_info_blocking() -> Vec<OpenWindowInfo> {
    #[cfg(windows)]
    {
        use std::collections::HashMap;
        use sysinfo::{ProcessesToUpdate, System};
        use windows::core::BOOL;
        use windows::Win32::Foundation::{HWND, LPARAM, RECT};
        use windows::Win32::UI::WindowsAndMessaging::{
            EnumWindows, GetWindowRect, GetWindowTextW, GetWindowThreadProcessId, IsIconic,
            IsWindowVisible,
        };

        struct RawWin {
            hwnd: isize,
            pid: u32,
            title: String,
            minimized: bool,
            width: i32,
            height: i32,
        }

        unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
            if IsWindowVisible(hwnd).as_bool() {
                let mut buf = [0u16; 256];
                let len = GetWindowTextW(hwnd, &mut buf);
                if len > 0 {
                    let title = String::from_utf16_lossy(&buf[..len as usize])
                        .trim()
                        .to_string();
                    let ignored = title.is_empty()
                        || title.starts_with("J.A.R.V.I.S.")
                        || title == "Program Manager"
                        || title == "Windows Input Experience"
                        || title == "Default IME"
                        || title == "MSCTFIME UI";
                    if !ignored {
                        let minimized = IsIconic(hwnd).as_bool();
                        let mut rect = RECT::default();
                        let _ = GetWindowRect(hwnd, &mut rect);
                        let width = (rect.right - rect.left).max(0);
                        let height = (rect.bottom - rect.top).max(0);
                        if minimized || (width >= 140 && height >= 90) {
                            let mut pid = 0u32;
                            let _ = GetWindowThreadProcessId(hwnd, Some(&mut pid));
                            let list = &mut *(lparam.0 as *mut Vec<RawWin>);
                            if list.len() < 32 {
                                list.push(RawWin {
                                    hwnd: hwnd.0 as isize,
                                    pid,
                                    title,
                                    minimized,
                                    width,
                                    height,
                                });
                            }
                        }
                    }
                }
            }
            BOOL(1)
        }

        let mut raw: Vec<RawWin> = Vec::new();
        unsafe {
            let _ = EnumWindows(
                Some(enum_proc),
                LPARAM(&mut raw as *mut Vec<RawWin> as isize),
            );
        }

        let mut sys = System::new();
        sys.refresh_processes(ProcessesToUpdate::All, true);
        let pid_map: HashMap<u32, String> = sys
            .processes()
            .iter()
            .map(|(pid, proc_)| (pid.as_u32(), proc_.name().to_string_lossy().to_string()))
            .collect();

        let mut out = Vec::new();
        for w in raw {
            let process_name = pid_map
                .get(&w.pid)
                .cloned()
                .unwrap_or_else(|| "app.exe".to_string());
            if process_name.eq_ignore_ascii_case("jarvis.exe") {
                continue;
            }
            out.push(OpenWindowInfo {
                hwnd: w.hwnd,
                pid: w.pid,
                process_name,
                title: w.title,
                minimized: w.minimized,
                width: w.width,
                height: w.height,
            });
            if out.len() >= 25 {
                break;
            }
        }
        out
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

pub(crate) fn match_target_window<'a>(
    query: &str,
    wins: &'a [OpenWindowInfo],
) -> Option<&'a OpenWindowInfo> {
    let q = query.trim().to_ascii_lowercase();
    if q.is_empty() || wins.is_empty() {
        return None;
    }

    const BROWSERS: &[&str] = &[
        "firefox", "chrome", "msedge", "brave", "opera", "vivaldi", "zen", "arc",
    ];
    const EDITORS: &[&str] = &[
        "code",
        "cursor",
        "windsurf",
        "devenv",
        "idea64",
        "pycharm",
        "rider",
        "zed",
        "sublime_text",
        "notepad++",
    ];
    const NON_GAMES: &[&str] = &[
        "firefox",
        "chrome",
        "msedge",
        "brave",
        "opera",
        "vivaldi",
        "zen",
        "arc",
        "code",
        "cursor",
        "windsurf",
        "devenv",
        "explorer",
        "cmd",
        "powershell",
        "pwsh",
        "windowsterminal",
        "wt",
        "notepad",
        "discord",
        "whatsapp",
        "telegram",
        "spotify",
        "slack",
        "teams",
        "ms-teams",
        "taskmgr",
        "applicationframehost",
        "systemsettings",
        "obs64",
        "jarvis",
        "steamwebhelper",
        "epicgameslauncher",
    ];
    const KNOWN_GAME_HINTS: &[&str] = &[
        "minecraft",
        "javaw",
        "tlauncher",
        "prismlauncher",
        "lunar",
        "gta",
        "fivem",
        "roblox",
        "valorant",
        "riot",
        "cs2",
        "csgo",
        "dota",
        "league",
        "fortnite",
        "elden",
        "cyberpunk",
        "witcher",
        "red dead",
        "rdr2",
        "mumu",
        "bluestacks",
        "ldplayer",
        "nox",
        "retroarch",
        "dolphin",
        "rpcs3",
        "pcsx2",
        "yuzu",
        "ryujinx",
        "cemu",
        "ppsspp",
        "unity",
        "unreal",
        "terraria",
        "stardew",
        "hollow knight",
        "celeste",
        "balatro",
        "palworld",
        "apex",
        "overwatch",
        "genshin",
        "honkai",
        "wuthering",
        "pubg",
        "warzone",
        "fifa",
        "fc24",
        "fc25",
        "rocket league",
        "baldurs",
        "skyrim",
        "fallout",
        "diablo",
        "path of exile",
        "osu",
        "geometry dash",
        "among us",
        "lethal company",
        "helldivers",
        "dark souls",
        "sekiro",
        "subnautica",
        "factorio",
        "rimworld",
        "civilization",
        "sims",
    ];

    let wants_browser = q.contains("navegador") || q.contains("browser");
    let wants_editor = q.contains("vscode")
        || q.contains("vs code")
        || q.contains("visual studio")
        || q.contains("editor");
    let wants_game = q.contains("jogo")
        || q.contains("game")
        || q.contains("partida")
        || q.contains("emulador")
        || q.contains("emulator");

    let stop_words = [
        "o", "a", "os", "as", "meu", "minha", "janela", "tela", "app", "do", "da", "de", "no", "na",
    ];
    let tokens: Vec<&str> = q
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() >= 2 && !stop_words.contains(t))
        .collect();

    let mut best: Option<(i32, &OpenWindowInfo)> = None;

    for win in wins {
        let proc_lower = win.process_name.to_ascii_lowercase();
        let stem = proc_lower.strip_suffix(".exe").unwrap_or(&proc_lower);
        let title_lower = win.title.to_ascii_lowercase();
        let combined = format!("{stem} {proc_lower} {title_lower}");
        let mut score = 0i32;

        if title_lower == q {
            score += 140;
        }
        if stem == q || proc_lower == q {
            score += 130;
        }
        if proc_lower.contains(&q) || stem.contains(&q) {
            score += 95;
        }
        if title_lower.contains(&q) {
            score += 90;
        }

        if !tokens.is_empty() {
            let matched_tokens = tokens.iter().filter(|t| combined.contains(**t)).count();
            if matched_tokens == tokens.len() {
                score += 75;
            } else if matched_tokens > 0 {
                score += (matched_tokens as i32) * 25;
            }
        }

        if wants_browser
            && BROWSERS
                .iter()
                .any(|b| stem.contains(b) || title_lower.contains(b))
        {
            score += 85;
        }
        if wants_editor
            && (EDITORS.iter().any(|e| stem == *e || stem.contains(e))
                || title_lower.contains("visual studio code"))
        {
            score += 85;
        }
        if wants_game {
            let is_known_game = KNOWN_GAME_HINTS
                .iter()
                .any(|g| stem.contains(g) || title_lower.contains(g));
            if is_known_game {
                score += 115;
            } else {
                let is_non_game = NON_GAMES
                    .iter()
                    .any(|ng| stem == *ng || stem.starts_with(ng));
                if !is_non_game && (win.minimized || (win.width >= 500 && win.height >= 350)) {
                    score += 55;
                }
            }
        }

        if score > 0 && !win.minimized {
            score += 5;
        }

        if score > 0 {
            match best {
                Some((best_score, _)) if score <= best_score => {}
                _ => best = Some((score, win)),
            }
        }
    }

    best.map(|(_, w)| w)
}

#[cfg(windows)]
fn capture_window_printwindow(hwnd_raw: isize) -> Result<(u32, u32, u32, u32, String), String> {
    use windows::Win32::Foundation::{HWND, RECT};
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits,
        ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
    };
    use windows::Win32::Storage::Xps::{PrintWindow, PRINT_WINDOW_FLAGS};
    use windows::Win32::UI::WindowsAndMessaging::GetWindowRect;

    let hwnd = HWND(hwnd_raw as *mut std::ffi::c_void);
    let mut rect = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut rect) }.map_err(|e| format!("GetWindowRect falhou: {e}"))?;

    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    if width < 32 || height < 32 {
        return Err(format!(
            "Janela com dimensões muito pequenas ({width}x{height})."
        ));
    }

    unsafe {
        let screen_dc = GetDC(None);
        if screen_dc.is_invalid() {
            return Err("GetDC(None) retornou nulo.".to_string());
        }
        let mem_dc = CreateCompatibleDC(Some(screen_dc));
        if mem_dc.is_invalid() {
            let _ = ReleaseDC(None, screen_dc);
            return Err("CreateCompatibleDC falhou.".to_string());
        }
        let hbmp = CreateCompatibleBitmap(screen_dc, width, height);
        if hbmp.is_invalid() {
            let _ = DeleteDC(mem_dc);
            let _ = ReleaseDC(None, screen_dc);
            return Err("CreateCompatibleBitmap falhou.".to_string());
        }

        let prev = SelectObject(mem_dc, hbmp.into());
        // PW_RENDERFULLCONTENT = 2 captures DirectComposition / hardware-accelerated windows behind others
        let mut printed = PrintWindow(hwnd, mem_dc, PRINT_WINDOW_FLAGS(2)).as_bool();
        if !printed {
            printed = PrintWindow(hwnd, mem_dc, PRINT_WINDOW_FLAGS(0)).as_bool();
        }
        // MSDN requires deselecting the bitmap before GetDIBits
        SelectObject(mem_dc, prev);

        if !printed {
            let _ = DeleteObject(hbmp.into());
            let _ = DeleteDC(mem_dc);
            let _ = ReleaseDC(None, screen_dc);
            return Err("PrintWindow retornou falso para esta janela.".to_string());
        }

        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut bgra = vec![0u8; (width as usize) * (height as usize) * 4];
        let lines = GetDIBits(
            screen_dc,
            hbmp,
            0,
            height as u32,
            Some(bgra.as_mut_ptr() as *mut _),
            &mut bmi,
            DIB_RGB_COLORS,
        );

        let _ = DeleteObject(hbmp.into());
        let _ = DeleteDC(mem_dc);
        let _ = ReleaseDC(None, screen_dc);

        if lines == 0 {
            return Err("GetDIBits falhou ao extrair pixels da janela.".to_string());
        }

        // Detect all-black DirectX/Vulkan/OpenGL frames so caller falls back to foreground DXGI
        let total_px = (width as usize) * (height as usize);
        let step = (total_px / 600).max(1);
        let mut non_black = 0usize;
        let mut sampled = 0usize;
        let mut idx = 0usize;
        while idx < total_px {
            let off = idx * 4;
            let b = bgra[off] as u16;
            let g = bgra[off + 1] as u16;
            let r = bgra[off + 2] as u16;
            if r + g + b > 24 {
                non_black += 1;
            }
            sampled += 1;
            idx += step;
        }
        if sampled > 0 && non_black * 100 / sampled < 3 {
            return Err(
                "PrintWindow retornou quadro preto (possível jogo DirectX/Vulkan/OpenGL)."
                    .to_string(),
            );
        }

        encode_bgra_to_jpeg_base64(&bgra, width as u32, height as u32, (width as usize) * 4)
    }
}

pub(crate) type WindowCapturePayload = ((u32, u32, u32, u32, String), String, &'static str);

pub(crate) fn capture_target_window_blocking(
    query: &str,
    open_wins: &[OpenWindowInfo],
) -> Result<WindowCapturePayload, String> {
    let Some(win) = match_target_window(query, open_wins) else {
        let available: Vec<String> = open_wins
            .iter()
            .take(12)
            .map(|w| format!("[{}] {}", w.process_name, w.title))
            .collect();
        return Err(format!(
            "Não encontrei nenhuma janela aberta correspondente a '{query}'. Janelas abertas no momento: {}",
            if available.is_empty() {
                "nenhuma".to_string()
            } else {
                available.join(" | ")
            }
        ));
    };

    let matched_label = format!("[{}] {}", win.process_name, win.title);

    #[cfg(windows)]
    {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{
            BringWindowToTop, SetForegroundWindow, ShowWindow, SW_RESTORE,
        };

        // 1) Try background capture with zero focus steal if window is not minimized
        if !win.minimized {
            if let Ok(shot) = capture_window_printwindow(win.hwnd) {
                return Ok((
                    shot,
                    matched_label,
                    "PrintWindow (em segundo plano, sem trocar foco)",
                ));
            }
        }

        // 2) Minimized window or DirectX/Vulkan game -> restore/focus briefly & capture via DXGI/GDI
        let hwnd = HWND(win.hwnd as *mut std::ffi::c_void);
        unsafe {
            if win.minimized {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            }
            let _ = BringWindowToTop(hwnd);
            let _ = SetForegroundWindow(hwnd);
        }
        std::thread::sleep(Duration::from_millis(280));

        // Retry PrintWindow first (in case it was just minimized and is a regular desktop app)
        if win.minimized {
            if let Ok(shot) = capture_window_printwindow(win.hwnd) {
                return Ok((
                    shot,
                    matched_label,
                    "PrintWindow (após restaurar janela minimizada)",
                ));
            }
        }

        let shot = capture_desktop_screen_blocking()?;
        Ok((
            shot,
            matched_label,
            "DXGI/GDI (janela trazida para primeiro plano)",
        ))
    }
    #[cfg(not(windows))]
    {
        let _ = matched_label;
        Err("Captura de janela específica disponível apenas no Windows.".to_string())
    }
}

pub(crate) fn screen_click_blocking(
    x: i32,
    y: i32,
    button: &str,
    double_click: bool,
) -> Result<String, String> {
    #[cfg(windows)]
    {
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
            MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEINPUT,
        };
        use windows::Win32::UI::WindowsAndMessaging::{
            GetSystemMetrics, SetCursorPos, SM_CXSCREEN,
        };

        unsafe {
            let real_w = GetSystemMetrics(SM_CXSCREEN).max(1);
            let cap_w = real_w.min(1280) as f64;
            let scale = real_w as f64 / cap_w;
            let rx = (x as f64 * scale).round() as i32;
            let ry = (y as f64 * scale).round() as i32;

            SetCursorPos(rx, ry).map_err(|e| format!("Falha ao mover cursor: {e}"))?;
            std::thread::sleep(Duration::from_millis(30));

            let is_right = button.eq_ignore_ascii_case("right");
            let down_flag = if is_right {
                MOUSEEVENTF_RIGHTDOWN
            } else {
                MOUSEEVENTF_LEFTDOWN
            };
            let up_flag = if is_right {
                MOUSEEVENTF_RIGHTUP
            } else {
                MOUSEEVENTF_LEFTUP
            };

            let clicks = if double_click { 2 } else { 1 };
            for i in 0..clicks {
                let inputs = [
                    INPUT {
                        r#type: INPUT_MOUSE,
                        Anonymous: INPUT_0 {
                            mi: MOUSEINPUT {
                                dwFlags: down_flag,
                                ..Default::default()
                            },
                        },
                    },
                    INPUT {
                        r#type: INPUT_MOUSE,
                        Anonymous: INPUT_0 {
                            mi: MOUSEINPUT {
                                dwFlags: up_flag,
                                ..Default::default()
                            },
                        },
                    },
                ];
                let sent = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
                if sent == 0 {
                    return Err("Win32 SendInput não conseguiu enviar o clique.".to_string());
                }
                if i + 1 < clicks {
                    std::thread::sleep(Duration::from_millis(65));
                }
            }

            Ok(format!(
                "Clique ({button}{}) executado na tela em ({rx},{ry}).",
                if double_click { ", duplo" } else { "" }
            ))
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (x, y, button, double_click);
        Err("Controle de mouse disponível apenas no Windows.".to_string())
    }
}

#[cfg(windows)]
fn send_vk(vk: u16, key_up: bool) {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
        VIRTUAL_KEY,
    };
    let input = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                dwFlags: if key_up {
                    KEYEVENTF_KEYUP
                } else {
                    KEYBD_EVENT_FLAGS(0)
                },
                ..Default::default()
            },
        },
    };
    unsafe {
        let _ = SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
    }
}

pub(crate) fn screen_type_blocking(text: &str, press_enter: bool) -> Result<String, String> {
    #[cfg(windows)]
    {
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
            KEYEVENTF_UNICODE, VIRTUAL_KEY,
        };

        let mut inputs: Vec<INPUT> = Vec::with_capacity(text.len() * 2 + 2);
        for unit in text.encode_utf16() {
            inputs.push(INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(0),
                        wScan: unit,
                        dwFlags: KEYEVENTF_UNICODE,
                        ..Default::default()
                    },
                },
            });
            inputs.push(INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(0),
                        wScan: unit,
                        dwFlags: KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
                        ..Default::default()
                    },
                },
            });
        }

        if !inputs.is_empty() {
            unsafe {
                let sent = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
                if sent == 0 {
                    return Err("Win32 SendInput não conseguiu digitar o texto.".to_string());
                }
            }
        }

        if press_enter {
            std::thread::sleep(Duration::from_millis(35));
            send_vk(0x0D, false);
            send_vk(0x0D, true);
        }

        Ok(format!(
            "Texto digitado na janela em foco{}.",
            if press_enter {
                " e Enter pressionado"
            } else {
                ""
            }
        ))
    }
    #[cfg(not(windows))]
    {
        let _ = (text, press_enter);
        Err("Digitação global disponível apenas no Windows.".to_string())
    }
}

pub(crate) fn parse_hotkey_vks(raw: &str) -> Result<Vec<u16>, String> {
    let norm = raw.trim();
    if norm.is_empty() {
        return Err("Atalho vazio.".to_string());
    }
    let expanded = match norm.to_ascii_lowercase().as_str() {
        "^c" => "ctrl+c",
        "^v" => "ctrl+v",
        "^a" => "ctrl+a",
        "^z" => "ctrl+z",
        "^s" => "ctrl+s",
        "^w" => "ctrl+w",
        "^t" => "ctrl+t",
        "^l" => "ctrl+l",
        "%{tab}" => "alt+tab",
        "%{f4}" => "alt+f4",
        "{enter}" => "enter",
        "{esc}" => "esc",
        "{tab}" => "tab",
        "{backspace}" => "backspace",
        "{delete}" => "delete",
        _ => norm,
    };

    let mut vks = Vec::new();
    for part in expanded.split('+').map(str::trim).filter(|s| !s.is_empty()) {
        let lower = part.to_ascii_lowercase();
        let vk = match lower.as_str() {
            "ctrl" | "control" => 0x11,
            "alt" => 0x12,
            "shift" => 0x10,
            "win" | "super" | "meta" => 0x5B,
            "enter" | "return" => 0x0D,
            "esc" | "escape" => 0x1B,
            "tab" => 0x09,
            "space" | "espaco" | "espaço" => 0x20,
            "backspace" => 0x08,
            "delete" | "del" => 0x2E,
            "up" | "arrowup" => 0x26,
            "down" | "arrowdown" => 0x28,
            "left" | "arrowleft" => 0x25,
            "right" | "arrowright" => 0x27,
            "home" => 0x24,
            "end" => 0x23,
            "pageup" => 0x21,
            "pagedown" => 0x22,
            "f1" => 0x70,
            "f2" => 0x71,
            "f3" => 0x72,
            "f4" => 0x73,
            "f5" => 0x74,
            "f6" => 0x75,
            "f7" => 0x76,
            "f8" => 0x77,
            "f9" => 0x78,
            "f10" => 0x79,
            "f11" => 0x7A,
            "f12" => 0x7B,
            s if s.len() == 1 => {
                let b = s.as_bytes()[0].to_ascii_uppercase();
                if b.is_ascii_alphanumeric() {
                    b as u16
                } else {
                    return Err(format!("Tecla não suportada no atalho: '{part}'"));
                }
            }
            _ => return Err(format!("Tecla desconhecida no atalho: '{part}'")),
        };
        vks.push(vk);
    }

    if vks.is_empty() {
        Err("Nenhuma tecla válida no atalho.".to_string())
    } else {
        Ok(vks)
    }
}

pub(crate) fn screen_hotkey_blocking(keys: &str) -> Result<String, String> {
    #[cfg(windows)]
    {
        let vks = parse_hotkey_vks(keys)?;
        for &vk in &vks {
            send_vk(vk, false);
        }
        std::thread::sleep(Duration::from_millis(35));
        for &vk in vks.iter().rev() {
            send_vk(vk, true);
        }
        Ok(format!("Atalho '{keys}' enviado nativamente."))
    }
    #[cfg(not(windows))]
    {
        let _ = keys;
        Err("Atalhos globais disponíveis apenas no Windows.".to_string())
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardContent {
    pub kind: String,
    pub text: Option<String>,
    pub image_base64: Option<String>,
    pub image_mime: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

pub(crate) fn dib_bytes_to_bgra(dib: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    if dib.len() < 40 {
        return Err("Cabeçalho DIB inválido na área de transferência.".to_string());
    }
    let header_size = u32::from_le_bytes([dib[0], dib[1], dib[2], dib[3]]) as usize;
    let width_i32 = i32::from_le_bytes([dib[4], dib[5], dib[6], dib[7]]);
    let height_i32 = i32::from_le_bytes([dib[8], dib[9], dib[10], dib[11]]);
    let bpp = u16::from_le_bytes([dib[14], dib[15]]);
    let compression = u32::from_le_bytes([dib[16], dib[17], dib[18], dib[19]]);
    let clr_used = u32::from_le_bytes([dib[32], dib[33], dib[34], dib[35]]) as usize;

    if width_i32 <= 0 || height_i32 == 0 || width_i32 > 16_384 || height_i32.unsigned_abs() > 16_384
    {
        return Err("Dimensões inválidas na imagem da área de transferência.".to_string());
    }
    if bpp != 24 && bpp != 32 {
        return Err(format!("Formato de cor DIB não suportado ({bpp} bpp)."));
    }

    let width = width_i32 as u32;
    let height = height_i32.unsigned_abs();
    let top_down = height_i32 < 0;

    // BI_BITFIELDS (3) adds 12 bytes of channel masks right after a 40-byte BITMAPINFOHEADER
    let extra_masks = if header_size == 40 && compression == 3 {
        12
    } else {
        0
    };
    let palette_bytes = clr_used * 4;
    let pixel_offset = header_size + extra_masks + palette_bytes;
    let src_stride = ((width as usize) * (bpp as usize)).div_ceil(32) * 4;
    let needed = pixel_offset + src_stride * (height as usize);
    if dib.len() < needed {
        return Err("Dados de pixel incompletos na imagem da área de transferência.".to_string());
    }

    let mut bgra = vec![0u8; (width as usize) * (height as usize) * 4];
    for y in 0..(height as usize) {
        let src_y = if top_down {
            y
        } else {
            (height as usize) - 1 - y
        };
        let src_row =
            &dib[pixel_offset + src_y * src_stride..pixel_offset + (src_y + 1) * src_stride];
        let dst_row = &mut bgra[y * (width as usize) * 4..(y + 1) * (width as usize) * 4];
        if bpp == 32 {
            for x in 0..(width as usize) {
                dst_row[x * 4] = src_row[x * 4];
                dst_row[x * 4 + 1] = src_row[x * 4 + 1];
                dst_row[x * 4 + 2] = src_row[x * 4 + 2];
                dst_row[x * 4 + 3] = 255;
            }
        } else {
            for x in 0..(width as usize) {
                dst_row[x * 4] = src_row[x * 3];
                dst_row[x * 4 + 1] = src_row[x * 3 + 1];
                dst_row[x * 4 + 2] = src_row[x * 3 + 2];
                dst_row[x * 4 + 3] = 255;
            }
        }
    }

    Ok((width, height, bgra))
}

pub fn read_clipboard_blocking() -> Result<ClipboardContent, String> {
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::HGLOBAL;
        use windows::Win32::System::DataExchange::{
            CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
        };
        use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};

        const CF_DIB: u32 = 8;
        const CF_UNICODETEXT: u32 = 13;

        unsafe {
            // Brief retry in case another app holds clipboard momentarily
            let mut opened = false;
            for _ in 0..4 {
                if OpenClipboard(None).is_ok() {
                    opened = true;
                    break;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            if !opened {
                return Err(
                    "Não foi possível abrir a área de transferência do Windows.".to_string()
                );
            }

            let result = (|| -> Result<ClipboardContent, String> {
                // Prefer text when copied text is available, otherwise check image
                if IsClipboardFormatAvailable(CF_UNICODETEXT).is_ok() {
                    if let Ok(handle) = GetClipboardData(CF_UNICODETEXT) {
                        let hglobal = HGLOBAL(handle.0);
                        let size = GlobalSize(hglobal);
                        let ptr = GlobalLock(hglobal);
                        if !ptr.is_null() && size >= 2 {
                            let max_units = size / 2;
                            let slice = std::slice::from_raw_parts(ptr as *const u16, max_units);
                            let len = slice.iter().position(|&c| c == 0).unwrap_or(max_units);
                            let text = String::from_utf16_lossy(&slice[..len]);
                            let _ = GlobalUnlock(hglobal);
                            if !text.trim().is_empty() {
                                return Ok(ClipboardContent {
                                    kind: "text".to_string(),
                                    text: Some(text),
                                    image_base64: None,
                                    image_mime: None,
                                    width: None,
                                    height: None,
                                });
                            }
                        } else if !ptr.is_null() {
                            let _ = GlobalUnlock(hglobal);
                        }
                    }
                }

                if IsClipboardFormatAvailable(CF_DIB).is_ok() {
                    if let Ok(handle) = GetClipboardData(CF_DIB) {
                        let hglobal = HGLOBAL(handle.0);
                        let size = GlobalSize(hglobal);
                        let ptr = GlobalLock(hglobal);
                        if !ptr.is_null() && size >= 40 {
                            let bytes = std::slice::from_raw_parts(ptr as *const u8, size).to_vec();
                            let _ = GlobalUnlock(hglobal);
                            let (w, h, bgra) = dib_bytes_to_bgra(&bytes)?;
                            let (cw, ch, _, _, b64) =
                                encode_bgra_to_jpeg_base64(&bgra, w, h, (w as usize) * 4)?;
                            return Ok(ClipboardContent {
                                kind: "image".to_string(),
                                text: None,
                                image_base64: Some(b64),
                                image_mime: Some("image/jpeg".to_string()),
                                width: Some(cw),
                                height: Some(ch),
                            });
                        } else if !ptr.is_null() {
                            let _ = GlobalUnlock(hglobal);
                        }
                    }
                }

                Ok(ClipboardContent {
                    kind: "empty".to_string(),
                    text: None,
                    image_base64: None,
                    image_mime: None,
                    width: None,
                    height: None,
                })
            })();

            let _ = CloseClipboard();
            result
        }
    }
    #[cfg(not(windows))]
    {
        Ok(ClipboardContent {
            kind: "empty".to_string(),
            text: None,
            image_base64: None,
            image_mime: None,
            width: None,
            height: None,
        })
    }
}
