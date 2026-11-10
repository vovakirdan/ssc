import { useEffect, useId, useRef, type ReactNode } from 'react';
export function Modal({ title, children, onClose }: { title: string; children: ReactNode; onClose(): void }) {
  const id = useId(), panel = useRef<HTMLElement>(null), close = useRef(onClose); close.current = onClose;
  useEffect(() => {
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    panel.current?.querySelector<HTMLElement>('button, a, input, select, textarea')?.focus();
    const key = (event: KeyboardEvent) => {
      if (event.key === 'Escape') { event.preventDefault(); close.current(); }
      if (event.key !== 'Tab') return;
      const elements = Array.from(panel.current?.querySelectorAll<HTMLElement>('button:not(:disabled), a[href], input:not(:disabled), select:not(:disabled), textarea:not(:disabled)') || []);
      if (!elements.length) { event.preventDefault(); panel.current?.focus(); return; }
      if (event.shiftKey && document.activeElement === elements[0]) { event.preventDefault(); elements.at(-1)?.focus(); }
      else if (!event.shiftKey && document.activeElement === elements.at(-1)) { event.preventDefault(); elements[0].focus(); }
    };
    document.addEventListener('keydown', key);
    return () => { document.removeEventListener('keydown', key); previous?.focus(); };
  }, []);
  return <div className="ssc-modal-backdrop" onMouseDown={event => { if (event.target === event.currentTarget) onClose(); }}><section className="ssc-modal" role="dialog" aria-modal="true" aria-labelledby={id} tabIndex={-1} ref={panel}><header><h2 id={id}>{title}</h2><button className="ssc-icon-button" aria-label="Закрыть окно" onClick={onClose}>×</button></header>{children}</section></div>;
}
