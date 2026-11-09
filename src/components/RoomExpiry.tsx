import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

export default function RoomExpiry({ generation }: { generation: number }) {
  const [remaining, setRemaining] = useState<number | null>(null);
  useEffect(() => {
    let disposed = false;
    const update = async () => {
      try {
        const room = await invoke<{ generation: number; remaining_seconds: number } | null>('get_temporary_room_state');
        if (!disposed) setRemaining(room?.generation === generation ? room.remaining_seconds : null);
      } catch { /* The native session owns enforcement; UI never extends it. */ }
    };
    void update();
    const timer = setInterval(() => { void update(); }, 1000);
    return () => { disposed = true; clearInterval(timer); };
  }, [generation]);
  if (remaining === null) return null;
  return <p role={remaining <= 300 ? 'alert' : 'status'} className={remaining <= 60 ? 'text-red-300 text-sm' : 'text-slate-300 text-sm'}>Временный чат завершится через {Math.floor(remaining / 60)}:{String(remaining % 60).padStart(2, '0')}{remaining <= 60 ? '. Осталась одна минута.' : remaining <= 300 ? '. Осталось не больше пяти минут.' : ''}</p>;
}
