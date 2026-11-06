import { fireEvent, render, screen } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import ConnectionHub from './ConnectionHub';

it('offers manual roles and an active short-link action', () => {
  const onCreate = vi.fn(), onJoin = vi.fn(), onBack = vi.fn(), onLink = vi.fn();
  render(<ConnectionHub onCreate={onCreate} onJoin={onJoin} onBack={onBack} onLink={onLink} />);
  fireEvent.click(screen.getByRole('button', { name: /Создать приглашение/ }));
  fireEvent.click(screen.getByRole('button', { name: /Принять приглашение/ }));
  fireEvent.click(screen.getByRole('button', { name: 'Назад' }));
  expect(onCreate).toHaveBeenCalledOnce(); expect(onJoin).toHaveBeenCalledOnce(); expect(onBack).toHaveBeenCalledOnce();
  fireEvent.click(screen.getByRole('button', { name: 'Подключиться по короткой ссылке' }));
  expect(onLink).toHaveBeenCalledOnce();
  expect(screen.queryByText('Скоро')).not.toBeInTheDocument();
  expect(screen.queryByRole('textbox')).not.toBeInTheDocument();
});
