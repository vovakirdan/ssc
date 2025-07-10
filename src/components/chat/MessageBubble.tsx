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
export const MessageBubble: FC<Props> = ({msg, index = 0}) => {
  // Определяем, является ли это частью связанного сообщения
  const isPartOfGroup = msg.groupId && msg.partIndex !== undefined && msg.totalParts !== undefined;
  const isFirstPart = isPartOfGroup && msg.partIndex === 0;
  const isLastPart = isPartOfGroup && msg.partIndex === msg.totalParts! - 1;
  
  // Определяем стили для группировки
  const getGroupStyles = () => {
    if (!isPartOfGroup) return '';
    
    let styles = '';
    if (isFirstPart) {
      styles += ' rounded-t-2xl';
    } else {
      styles += ' rounded-none';
    }
    
    if (isLastPart) {
      styles += ' rounded-b-2xl';
    } else {
      styles += ' rounded-none';
    }
    
    // Убираем отступы между частями
    if (!isFirstPart) {
      styles += ' -mt-1';
    }
    
    return styles;
  };

  return (
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
        className={`max-w-[70%] p-3 ${getGroupStyles()} ${
          msg.isOwn
            ? 'bg-emerald-600 text-white'
            : 'bg-slate-700 text-white border-slate-600'
        }`}
      >
        <p className="text-sm break-words">{msg.text}</p>
        
        {/* Показываем информацию о части сообщения */}
        {isPartOfGroup && (
          <p
            className={`text-xs mt-1 ${
              msg.isOwn ? 'text-emerald-100' : 'text-slate-400'
            }`}
          >
            Часть {msg.partIndex! + 1} из {msg.totalParts}
          </p>
        )}
        
        {/* Показываем время только для последней части или одиночного сообщения */}
        {(isLastPart || !isPartOfGroup) && (
          <p
            className={`text-xs mt-1 ${
              msg.isOwn ? 'text-emerald-100' : 'text-slate-400'
            }`}
          >
            {msg.timestamp.toLocaleTimeString([], {hour: '2-digit', minute: '2-digit'})}
          </p>
        )}
      </Card>
    </motion.div>
  );
};
