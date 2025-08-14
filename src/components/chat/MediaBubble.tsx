import { Button } from '@/components/ui/button';
import type { FC } from 'react';

export interface MediaInfo {
  id: string;
  name: string;
  mime: string;
  size: number;
  dataUrl?: string;
  progress?: number;
}

interface Props {
  media: MediaInfo;
  isOwn: boolean;
}

/**
 * Компонент отображения медиа в сообщении.
 * Выделен отдельно, чтобы удобно расширять функциональность (сохранение, предпросмотр и т.д.).
 */
export const MediaBubble: FC<Props> = ({ media, isOwn }) => {
  const canPreviewImage = Boolean(media.dataUrl && media.mime.startsWith('image/'));
  const canDownload = Boolean(media.dataUrl);

  const handleSave = () => {
    // Простейшее сохранение через скрытую ссылку
    if (!media.dataUrl) return;
    const a = document.createElement('a');
    a.href = media.dataUrl;
    a.download = media.name || 'file';
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
  };

  return (
    <div className="space-y-2">
      <p className="text-xs opacity-90 break-all">{media.name}</p>

      {canPreviewImage ? (
        <img
          src={media.dataUrl}
          alt={media.name}
          className="max-h-64 rounded shadow"
        />
      ) : media.dataUrl ? (
        <a
          href={media.dataUrl}
          download={media.name}
          className="underline break-all"
        >
          Скачать файл
        </a>
      ) : (
        <p className="text-sm">Получение файла…</p>
      )}

      {typeof media.progress === 'number' && media.progress < 1 && (
        <p className={`text-xs ${isOwn ? 'text-emerald-100' : 'text-slate-400'}`}>
          Загрузка: {Math.round(media.progress * 100)}%
        </p>
      )}

      {canDownload && (
        <div className="pt-1">
          <Button
            type="button"
            size="sm"
            onClick={handleSave}
            className={isOwn ? 'bg-emerald-700 hover:bg-emerald-600' : 'bg-slate-600 hover:bg-slate-500'}
          >
            Сохранить
          </Button>
        </div>
      )}
    </div>
  );
};


