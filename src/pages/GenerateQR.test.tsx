import { StrictMode } from 'react';
import { render } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import GenerateQR from './GenerateQR';
const native = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: native.invoke }));
vi.mock('@/components/CircularText', () => ({ default: () => <span>Генерация</span> }));
it('does not start concurrent generation under StrictMode', () => {
  native.invoke.mockReturnValue(new Promise(() => {}));
  const { unmount } = render(<StrictMode><GenerateQR onBack={vi.fn()} onConnected={vi.fn()} autoGenerate /></StrictMode>);
  expect(native.invoke).toHaveBeenCalledTimes(1);
  expect(native.invoke).toHaveBeenCalledWith('generate_offer_with_candidates');
  unmount();
});
