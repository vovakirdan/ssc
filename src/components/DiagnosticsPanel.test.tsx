import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import DiagnosticsPanel from './DiagnosticsPanel';
const native = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: native.invoke }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => vi.fn()) }));
it('allows diagnostics to be enabled in the app and disables with an empty log', async () => {
  let enabled = false;
  native.invoke.mockImplementation(async (name: string, args?: { enabled: boolean }) => {
    if (name === 'get_session_state') return { generation: 1, phase: 'ice', route: null };
    if (name === 'set_diagnostics') enabled = args?.enabled ?? false;
    if (name === 'get_diagnostics') return { enabled, events: enabled ? [{ timestamp: 'now', session_id: 1, component: 'transport', stage: 'ice', outcome: 'started', code: null }] : [] };
  });
  render(<DiagnosticsPanel />);
  fireEvent.click(screen.getByRole('button', { name: 'Диагностика' }));
  const checkbox = screen.getByRole('checkbox', { name: 'Записывать события' });
  expect(checkbox).not.toBeChecked();
  fireEvent.click(checkbox);
  await waitFor(() => expect(checkbox).toBeChecked());
  expect(screen.getByRole('list', { name: 'События' })).toHaveTextContent('transport / ice');
  fireEvent.click(checkbox);
  await waitFor(() => expect(checkbox).not.toBeChecked());
  expect(screen.getByRole('list', { name: 'События' })).toBeEmptyDOMElement();
});
