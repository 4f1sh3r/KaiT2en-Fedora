export {};

declare global {
  interface Window {
    wizardApi: {
      mode: () => Promise<'install' | 'uninstall'>;
      start: () => void;
      answer: (value: string) => void;
      onEvent: (cb: (event: unknown) => void) => void;
      onProcessExit: (cb: (code: number) => void) => void;
    };
    windowApi: {
      minimize: () => void;
      toggleMaximize: () => void;
      close: () => void;
    };
  }
}

type WizardEvent =
  | { type: 'phase'; name: string; status: 'start' | 'done' }
  | { type: 'log'; phase: string; level: 'info' | 'warn'; text: string }
  | { type: 'question'; kind: 'continue' | 'purge' }
  | { type: 'error'; phase: string; message: string }
  | { type: 'done'; needsRelogin?: boolean };

type Mode = 'install' | 'uninstall';

const STEPS: Record<Mode, { id: string; label: string }[]> = {
  install: [
    { id: 'analysis', label: 'Analysis' },
    { id: 'purge', label: 'Purge' },
    { id: 'deploy', label: 'Deploy' },
  ],
  uninstall: [{ id: 'uninstall', label: 'Uninstall' }],
};

const WELCOME_COPY: Record<Mode, { title: string; paragraphs: string[]; beginLabel: string; danger?: boolean }> = {
  install: {
    title: 'Install react-drm',
    paragraphs: [
      'This replaces the existing Touch Bar interface. It analyzes your system, removes any conflicting tiny-dfr or mac-touchbar-plus installation (with your explicit confirmation), then builds and deploys react-drm.',
      'If your user needs to be added to the video or input groups, you will need to log out and back in afterward.',
      'Provided without warranty — used entirely at your own risk.',
    ],
    beginLabel: 'Begin Installation',
  },
  uninstall: {
    title: 'Uninstall react-drm',
    paragraphs: [
      'This removes the react-drm user service and udev rules and restores the firmware Touch Bar interface.',
      'Project files, npm dependencies, system packages and video/input group memberships are not removed.',
    ],
    beginLabel: 'Uninstall',
    danger: true,
  },
};

const QUESTION_COPY: Record<string, { title: string; body: string; confirmLabel: string; confirmValue: string; danger?: boolean }> = {
  continue: {
    title: 'Ready to deploy',
    body: 'No conflicting Touch Bar daemon was found. Continue installing dependencies and deploying react-drm?',
    confirmLabel: 'Continue',
    confirmValue: 'CONTINUE',
  },
  purge: {
    title: 'Remove the existing Touch Bar daemon',
    body: 'A conflicting tiny-dfr or mac-touchbar-plus installation was found. It will be stopped, disabled and removed before react-drm is deployed. This cannot be undone automatically.',
    confirmLabel: 'Purge and continue',
    confirmValue: 'PURGE',
    danger: true,
  },
};

let mode: Mode = 'install';
let settled = false;

function $(id: string): HTMLElement {
  return document.getElementById(id)!;
}

function showScreen(id: string): void {
  document.querySelectorAll('.screen').forEach(el => el.classList.remove('active'));
  $(id).classList.add('active');
}

function renderWelcome(): void {
  const copy = WELCOME_COPY[mode];
  $('welcome-title').textContent = copy.title;
  const body = $('welcome-body');
  body.innerHTML = '';
  for (const p of copy.paragraphs) {
    const el = document.createElement('p');
    el.textContent = p;
    body.appendChild(el);
  }
  const begin = $('begin-btn') as HTMLButtonElement;
  begin.textContent = copy.beginLabel;
  begin.className = copy.danger ? 'danger' : '';
}

function renderStepper(): void {
  const stepper = $('stepper');
  stepper.innerHTML = '';
  for (const step of STEPS[mode]) {
    const li = document.createElement('li');
    li.id = `step-${step.id}`;
    li.innerHTML = `<span class="dot">${STEPS[mode].indexOf(step) + 1}</span><span>${step.label}</span>`;
    stepper.appendChild(li);
  }
}

function setStepState(stepId: string, state: 'active' | 'done'): void {
  const steps = STEPS[mode];
  const idx = steps.findIndex(s => s.id === stepId);
  if (idx === -1) return;
  steps.forEach((s, i) => {
    const li = document.getElementById(`step-${s.id}`);
    if (!li) return;
    li.classList.remove('active', 'done');
    if (i < idx || (i === idx && state === 'done')) li.classList.add('done');
    else if (i === idx) li.classList.add('active');
  });
}

function appendLog(text: string, kind: 'info' | 'warn' | 'error' | 'phase' = 'info'): void {
  const log = $('log');
  const line = document.createElement('div');
  line.className = `log-line ${kind}`;
  line.textContent = text;
  log.appendChild(line);
  log.scrollTop = log.scrollHeight;
}

function showQuestion(kind: 'continue' | 'purge'): void {
  const copy = QUESTION_COPY[kind];
  $('question-title').textContent = copy.title;
  $('question-body').textContent = copy.body;
  const actions = $('question-actions');
  actions.innerHTML = '';

  const cancel = document.createElement('button');
  cancel.type = 'button';
  cancel.className = 'secondary';
  cancel.textContent = 'Cancel';
  cancel.addEventListener('click', () => {
    hideQuestion();
    window.wizardApi.answer('no');
  });

  const confirm = document.createElement('button');
  confirm.type = 'button';
  if (copy.danger) confirm.className = 'danger';
  confirm.textContent = copy.confirmLabel;
  confirm.addEventListener('click', () => {
    hideQuestion();
    window.wizardApi.answer(copy.confirmValue);
  });

  actions.appendChild(cancel);
  actions.appendChild(confirm);
  $('question-overlay').classList.remove('hidden');
}

function hideQuestion(): void {
  $('question-overlay').classList.add('hidden');
}

function showResult(ok: boolean, title: string, message: string): void {
  if (settled) return;
  settled = true;
  hideQuestion();
  const icon = $('result-icon');
  icon.className = ok ? 'ok' : 'error';
  icon.innerHTML = ok
    ? '<svg width="22" height="22" viewBox="0 0 24 24" fill="none"><path d="M5 13l4 4L19 7" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"/></svg>'
    : '<svg width="22" height="22" viewBox="0 0 24 24" fill="none"><path d="M6 6l12 12M18 6L6 18" stroke="currentColor" stroke-width="2.5" stroke-linecap="round"/></svg>';
  $('result-title').textContent = title;
  $('result-message').textContent = message;
  showScreen('screen-result');
}

function handleEvent(event: WizardEvent): void {
  switch (event.type) {
    case 'phase':
      setStepState(event.name, event.status === 'done' ? 'done' : 'active');
      appendLog(`${event.name} ${event.status === 'start' ? 'started' : 'finished'}`, 'phase');
      break;
    case 'log':
      appendLog(event.text, event.level);
      break;
    case 'question':
      showQuestion(event.kind);
      break;
    case 'error':
      appendLog(event.message, 'error');
      showResult(false, 'Something went wrong', event.message);
      break;
    case 'done':
      if (mode === 'install' && event.needsRelogin) {
        showResult(true, 'Installed — log out required', 'react-drm is enabled but was not started. Log out and back in to activate the new group memberships.');
      } else if (mode === 'install') {
        showResult(true, 'Installation complete', 'react-drm is active. No logout is required.');
      } else {
        showResult(true, 'Uninstalled', 'react-drm has been removed. The firmware Touch Bar interface is restored.');
      }
      break;
  }
}

async function main(): Promise<void> {
  mode = await window.wizardApi.mode();
  renderWelcome();
  renderStepper();

  $('begin-btn').addEventListener('click', () => {
    showScreen('screen-working');
    window.wizardApi.start();
  });

  window.wizardApi.onEvent(e => handleEvent(e as WizardEvent));
  window.wizardApi.onProcessExit(code => {
    if (!settled) showResult(false, 'Process exited unexpectedly', `The underlying script exited with code ${code} before reporting a result.`);
  });

  $('close-btn').addEventListener('click', () => window.windowApi.close());
  $('win-minimize').addEventListener('click', () => window.windowApi.minimize());
  $('win-maximize').addEventListener('click', () => window.windowApi.toggleMaximize());
  $('win-close').addEventListener('click', () => window.windowApi.close());
}

main();
