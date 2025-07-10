import {motion} from 'framer-motion';
import {Card} from '@/components/ui/card';
import type {FC} from 'react';
import type {Message} from '@/components/chat/types';

interface Props {
  msg: Message;
  index?: number;
}

/**
 * Single chat bubble with fade / slide animation.
 * Keeps styling concerns isolated from Chat.tsx.
 */
export const MessageBubble: FC<Props> = ({msg, index = 0}) => (
  <motion.div
    initial={{ opacity: 0, scale: 0.8, y: 20 }}
    animate={{ opacity: 1, scale: 1, y: 0 }}
    exit={{
      opacity: 0,
      scale: 0.3,
      filter: 'blur(8px)',
      y: -20,
      x: (Math.random() - 0.5) * 100,
      rotate: (Math.random() - 0.5) * 20
    }}
    transition={{ 
      duration: 0.6,
      ease: "easeInOut",
      delay: index * 0.05 // Небольшая задержка для stagger эффекта
    }}
    className={`flex ${msg.isOwn ? 'justify-end' : 'justify-start'}`}
  >
    <Card
      className={`max-w-[70%] p-3 rounded-2xl ${
        msg.isOwn
          ? 'bg-emerald-600 text-white'
          : 'bg-slate-700 text-white border-slate-600'
      }`}
    >
      <p className="text-sm break-words">{msg.text}</p>
      <p
        className={`text-xs mt-1 ${
          msg.isOwn ? 'text-emerald-100' : 'text-slate-400'
        }`}
      >
        {msg.timestamp.toLocaleTimeString([], {hour: '2-digit', minute: '2-digit'})}
      </p>
    </Card>
  </motion.div>
);
