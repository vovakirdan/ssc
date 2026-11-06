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
