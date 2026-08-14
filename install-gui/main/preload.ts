import { contextBridge, ipcRenderer } from 'electron';

contextBridge.exposeInMainWorld('wizardApi', {
  mode: () => ipcRenderer.invoke('wizard:mode'),
  start: () => ipcRenderer.send('wizard:start'),
  answer: (value: string) => ipcRenderer.send('wizard:answer', value),
  onEvent: (cb: (event: unknown) => void) => {
    ipcRenderer.on('wizard:event', (_event, data: unknown) => cb(data));
  },
  onProcessExit: (cb: (code: number) => void) => {
    ipcRenderer.on('wizard:process-exit', (_event, code: number) => cb(code));
  },
});

contextBridge.exposeInMainWorld('windowApi', {
  minimize: () => ipcRenderer.send('window:minimize'),
  toggleMaximize: () => ipcRenderer.send('window:toggleMaximize'),
  close: () => ipcRenderer.send('window:close'),
});
