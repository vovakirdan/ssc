import { createContext, useContext, useEffect, useState, type ReactNode } from 'react';
import './ui/chat.css';
export type ThemeMode = 'auto' | 'dark' | 'light';
const key = 'ssc-theme';
const valid = (value: unknown): value is ThemeMode => value === 'auto' || value === 'dark' || value === 'light';
function saved(): ThemeMode { try { const value = localStorage.getItem(key); return valid(value) ? value : 'auto'; } catch { return 'auto'; } }
function darkSystem() { return typeof matchMedia === 'function' ? matchMedia('(prefers-color-scheme: dark)').matches : true; }
const Theme = createContext<{ mode: ThemeMode; change(mode: ThemeMode): void }>({ mode: 'auto', change: () => undefined });
export function ThemeProvider({ children }: { children: ReactNode }) {
  const [mode, setMode] = useState<ThemeMode>(saved), [dark, setDark] = useState(darkSystem);
  useEffect(() => {
    if (typeof matchMedia !== 'function') return;
    const media = matchMedia('(prefers-color-scheme: dark)');
    const update = () => setDark(media.matches); media.addEventListener('change', update);
    return () => media.removeEventListener('change', update);
  }, []);
  const resolved = mode === 'auto' ? dark ? 'dark' : 'light' : mode;
  useEffect(() => {
    const element = document.documentElement, previous = element.getAttribute('data-ssc-theme');
    element.setAttribute('data-ssc-theme', resolved);
    return () => { if (element.getAttribute('data-ssc-theme') === resolved) { if (previous === null) element.removeAttribute('data-ssc-theme'); else element.setAttribute('data-ssc-theme', previous); } };
  }, [resolved]);
  const change = (next: ThemeMode) => {
    setMode(next);
    try { if (next === 'auto') localStorage.removeItem(key); else localStorage.setItem(key, next); } catch { /* Theme is cosmetic; browser storage can be disabled. */ }
  };
  return <Theme.Provider value={{ mode, change }}><div className="ssc-theme-root" data-ssc-theme={resolved}>{children}</div></Theme.Provider>;
}
export function ThemeControl() {
  const { mode, change } = useContext(Theme);
  return <label className="ssc-theme-control"><span className="ssc-sr-only">Тема</span><select aria-label="Тема" value={mode} onChange={event => { if (valid(event.target.value)) change(event.target.value); }}><option value="auto">Авто</option><option value="dark">Тёмная</option><option value="light">Светлая</option></select></label>;
}
