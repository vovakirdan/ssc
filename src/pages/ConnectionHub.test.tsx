import { fireEvent, render, screen } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import ConnectionHub from './ConnectionHub';

it('offers create and join roles with an unavailable short-link option', () => {
  const onCreate = vi.fn(), onJoin = vi.fn(), onBack = vi.fn();
  render(<ConnectionHub onCreate={onCreate} onJoin={onJoin} onBack={onBack} />);
  fireEvent.click(screen.getByRole('button', { name: /Создать приглашение/ }));
  fireEvent.click(screen.getByRole('button', { name: /Принять приглашение/ }));
  fireEvent.click(screen.getByRole('button', { name: 'Назад' }));
  expect(onCreate).toHaveBeenCalledOnce(); expect(onJoin).toHaveBeenCalledOnce(); expect(onBack).toHaveBeenCalledOnce();
  expect(screen.getByRole('button', { name: 'Подключиться по короткой ссылке' })).toBeDisabled();
  expect(screen.getByText(/Сервис ещё не подключён/)).toBeInTheDocument();
  expect(screen.queryByRole('textbox')).not.toBeInTheDocument();
});
