import type { ReactNode } from 'react';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import Settings from './Settings';

const native = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: native.invoke }));
vi.mock('@/components/AnimatedContent', () => ({ default: ({ children }: { children: ReactNode }) => <>{children}</> }));
vi.mock('@/components/FadeContent', () => ({ default: ({ children }: { children: ReactNode }) => <>{children}</> }));
vi.mock('@/components/text/ShinyText', () => ({ default: ({ text }: { text: string }) => <span>{text}</span> }));
const customServer = { id: 'personal', type: 'turn', url: 'turn:relay.example:3478', username: 'personal-user', credential: 'personal-password' };
beforeEach(() => {
  localStorage.clear();
  localStorage.setItem('ssc-settings', JSON.stringify({ servers: [customServer], offerTTL: 10 }));
  native.invoke.mockReset().mockResolvedValue(true);
  vi.stubGlobal('ResizeObserver', class { observe() {} unobserve() {} disconnect() {} });
});

it('keeps the chosen TURN server and credentials when checking and saving, with no fake refresh action', async () => {
  const onBack = vi.fn();
  const { container } = render(<Settings onBack={onBack} />);
  expect(screen.getByDisplayValue(customServer.url)).toBeInTheDocument();
  expect(screen.getByDisplayValue(customServer.username)).toBeInTheDocument();
  expect(screen.getByDisplayValue(customServer.credential)).toHaveAttribute('type', 'password');
  expect(container.querySelector('.lucide-cloud-download')).toBeNull();
  fireEvent.click(screen.getByRole('button', { name: 'Проверить' }));
  await waitFor(() => expect(native.invoke).toHaveBeenCalledWith('check_ice_server_availability', { config: customServer }));
  fireEvent.click(screen.getByRole('button', { name: 'Сохранить настройки' }));
  await waitFor(() => expect(onBack).toHaveBeenCalledOnce());
  expect(native.invoke).toHaveBeenCalledWith('set_ice_servers', { servers: [{ ...customServer, status: 'valid' }] });
  expect(JSON.parse(localStorage.getItem('ssc-settings') || '{}').servers).toEqual([{ ...customServer, status: 'valid' }]);
  expect(localStorage.getItem('ssc-settings')).not.toContain('openrelay');
});

it('keeps manual TURN editing available and persists a chosen replacement', async () => {
  render(<Settings onBack={vi.fn()} />);
  fireEvent.change(screen.getByDisplayValue(customServer.url), { target: { value: 'turn:other.example:3478' } });
  fireEvent.change(screen.getByDisplayValue(customServer.username), { target: { value: 'other-user' } });
  fireEvent.change(screen.getByDisplayValue(customServer.credential), { target: { value: 'other-password' } });
  fireEvent.click(screen.getByRole('button', { name: 'Сохранить настройки' }));
  await waitFor(() => expect(native.invoke).toHaveBeenCalledWith('set_ice_servers', { servers: [{ ...customServer, url: 'turn:other.example:3478', username: 'other-user', credential: 'other-password', status: 'idle' }] }));
});

it('switches to project TURN without replacing custom server credentials', async () => {
  const onBack = vi.fn(); render(<Settings onBack={onBack} />);
  fireEvent.change(screen.getByRole('combobox', { name: 'Способ подключения' }), { target: { value: 'project' } });
  fireEvent.click(screen.getByRole('button', { name: 'Сохранить настройки' }));
  await waitFor(() => expect(onBack).toHaveBeenCalledOnce());
  expect(native.invoke).toHaveBeenCalledWith('set_network_mode', { mode: 'project' });
  expect(native.invoke).not.toHaveBeenCalledWith('set_ice_servers', expect.anything());
  expect(JSON.parse(localStorage.getItem('ssc-settings') || '{}')).toMatchObject({ networkMode: 'project', servers: [customServer] });
});
it('explicit no TURN mode needs no personal activation or custom servers', async () => {
  localStorage.setItem('ssc-settings', JSON.stringify({ networkMode: 'disabled', servers: [], offerTTL: 5 }));
  render(<Settings onBack={vi.fn()} />);
  fireEvent.click(screen.getByRole('button', { name: 'Сохранить настройки' }));
  await waitFor(() => expect(native.invoke).toHaveBeenCalledWith('set_network_mode', { mode: 'disabled' }));
  expect(JSON.parse(localStorage.getItem('ssc-settings') || '{}').networkMode).toBe('disabled');
});
it('deletes local activation only after an explicit confirmation', async () => {
  render(<Settings onBack={vi.fn()} />);
  fireEvent.change(screen.getByRole('combobox', { name: 'Способ подключения' }), { target: { value: 'project' } });
  fireEvent.click(screen.getByRole('button', { name: 'Сбросить локальную активацию' }));
  expect(native.invoke).not.toHaveBeenCalledWith('reset_access');
  expect(await screen.findByText(/Другие копии ключа и допуск на сервере этим действием не отзываются/)).toBeInTheDocument();
  fireEvent.click(await screen.findByRole('button', { name: 'Удалить локальный допуск' }));
  await waitFor(() => expect(native.invoke).toHaveBeenCalledWith('reset_access'));
  expect(JSON.parse(localStorage.getItem('ssc-settings') || '{}').servers).toEqual([customServer]);
});
it('shows and copies only the active installation reference for operator revocation', async () => {
  const reference = 'a'.repeat(43);
  const copy = vi.fn().mockResolvedValue(undefined);
  const original = Object.getOwnPropertyDescriptor(navigator, 'clipboard');
  Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText: copy } });
  try {
    native.invoke.mockImplementation(async (name: string) => name === 'get_access_status' ? { state: 'active', access_reference: reference } : true);
    render(<Settings onBack={vi.fn()} />);
    fireEvent.change(screen.getByRole('combobox', { name: 'Способ подключения' }), { target: { value: 'project' } });
    expect(await screen.findByRole('textbox', { name: 'Номер доступа' })).toHaveValue(reference);
    fireEvent.click(screen.getByRole('button', { name: 'Скопировать номер доступа' }));
    await waitFor(() => expect(copy).toHaveBeenCalledExactlyOnceWith(reference));
  } finally {
    if (original) Object.defineProperty(navigator, 'clipboard', original);
    else Reflect.deleteProperty(navigator, 'clipboard');
  }
});
it('does not display a reference from an inactive or failed activation', async () => {
  native.invoke.mockImplementation(async (name: string) => name === 'get_access_status' ? { state: 'inactive', access_reference: 'old-reference' } : true);
  render(<Settings onBack={vi.fn()} />);
  fireEvent.change(screen.getByRole('combobox', { name: 'Способ подключения' }), { target: { value: 'project' } });
  await screen.findByText('Для первого запуска нужен персональный установщик с активацией.');
  expect(screen.queryByRole('textbox', { name: 'Номер доступа' })).not.toBeInTheDocument();
  expect(screen.queryByRole('button', { name: 'Скопировать номер доступа' })).not.toBeInTheDocument();
});
