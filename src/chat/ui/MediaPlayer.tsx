import { useLayoutEffect, useRef } from 'react';
/** A revoked Blob URL alone does not stop a decoder which already buffered audio. */
export function MediaPlayer({ kind, url, label }: { kind: 'audio' | 'video'; url: string; label: string }) {
  const media = useRef<HTMLMediaElement | null>(null);
  useLayoutEffect(() => {
    const element = media.current;
    const stop = () => {
      if (!element) return;
      try { element.pause(); } catch { /* A closing renderer may already have retired it. */ }
      element.removeAttribute('src');
      try { element.load(); } catch { /* The source is detached even if teardown rejects load. */ }
    };
    window.addEventListener('pagehide', stop);
    return () => { window.removeEventListener('pagehide', stop); stop(); };
  }, [url, kind]);
  return kind === 'audio' ? <audio key={url} ref={element => { media.current = element; }} controls preload="none" src={url} aria-label={label} />
    : <video key={url} ref={element => { media.current = element; }} controls preload="none" playsInline src={url} aria-label={label} />;
}
