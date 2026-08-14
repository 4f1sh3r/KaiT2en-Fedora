import { app, BrowserWindow, ipcMain, screen } from 'electron';
import * as path from 'node:path';
import * as readline from 'node:readline';
import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process';

type Mode = 'install' | 'uninstall';

function parseMode(): Mode {
  const arg = process.argv.find(a => a.startsWith('--mode='));
  return arg?.slice('--mode='.length) === 'uninstall' ? 'uninstall' : 'install';
}

// install-gui always lives at <repo>/install-gui, so this is a safe default
// for local runs; the shell scripts that actually launch this app set
// REACT_DRM_REPO_DIR explicitly since they already know their own root.
const REPO_ROOT = process.env.REACT_DRM_REPO_DIR ?? path.join(__dirname, '..', '..', '..');
const MODE = parseMode();
const SCRIPT_ARGS = MODE === 'uninstall' ? ['uninstall.sh', 'uninstall', '--gui'] : ['install.sh', 'install', '--gui'];

let mainWindow: BrowserWindow | null = null;
let child: ChildProcessWithoutNullStreams | null = null;

function createWindow(): void {
  const { width: screenW, height: screenH } = screen.getPrimaryDisplay().workAreaSize;
  const win = new BrowserWindow({
    width: Math.round(screenW / 2),
    height: Math.round(screenH / 2),
    minWidth: 640,
    minHeight: 480,
    center: true,
    title: MODE === 'uninstall' ? 'Uninstall react-drm' : 'Install react-drm',
    frame: false,
    backgroundColor: '#0d0f14',
    webPreferences: {
      preload: path.join(__dirname, 'preload.js'),
      contextIsolation: true,
      nodeIntegration: false,
    },
  });
  mainWindow = win;
  win.on('closed', () => {
    mainWindow = null;
    child?.kill();
    child = null;
  });
  win.loadFile(path.join(__dirname, '..', 'renderer', 'index.html'));
}

function startProcess(): void {
  if (child) return;
  child = spawn('bash', SCRIPT_ARGS, { cwd: REPO_ROOT, env: process.env });

  const stdout = readline.createInterface({ input: child.stdout });
  stdout.on('line', line => {
    if (!line.trim()) return;
    try {
      const event = JSON.parse(line);
      mainWindow?.webContents.send('wizard:event', event);
    } catch {
      mainWindow?.webContents.send('wizard:event', { type: 'log', phase: MODE, level: 'info', text: line });
    }
  });

  const stderr = readline.createInterface({ input: child.stderr });
  stderr.on('line', line => {
    if (!line.trim()) return;
    mainWindow?.webContents.send('wizard:event', { type: 'log', phase: MODE, level: 'warn', text: line });
  });

  child.on('exit', code => {
    mainWindow?.webContents.send('wizard:process-exit', code ?? -1);
    child = null;
  });
}

ipcMain.on('wizard:start', () => startProcess());
ipcMain.on('wizard:answer', (_event, answer: string) => {
  child?.stdin.write(`${JSON.stringify({ answer })}\n`);
});

ipcMain.handle('wizard:mode', () => MODE);

ipcMain.on('window:minimize', () => mainWindow?.minimize());
ipcMain.on('window:toggleMaximize', () => {
  if (!mainWindow) return;
  if (mainWindow.isMaximized()) mainWindow.unmaximize();
  else mainWindow.maximize();
});
ipcMain.on('window:close', () => mainWindow?.close());

app.whenReady().then(createWindow);

app.on('window-all-closed', () => {
  child?.kill();
  if (process.platform !== 'darwin') app.quit();
});
