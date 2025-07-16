import * as React from "react"

import { cn } from "@/lib/utils"

// Интерфейс для расширенных пропсов
interface InputProps extends React.ComponentProps<"input"> {
  autoFocus?: boolean;
  autoResize?: boolean;
  maxRows?: number;
}

const Input = React.forwardRef<HTMLInputElement, InputProps>(
  ({ className, type, autoFocus = false, autoResize = false, maxRows = 3, onChange, ...props }, ref) => {
    const inputRef = React.useRef<HTMLInputElement>(null);

    // Объединяем refs
    React.useImperativeHandle(ref, () => inputRef.current!);

    // Автофокус при монтировании
    React.useEffect(() => {
      if (autoFocus && inputRef.current) {
        inputRef.current.focus();
      }
    }, [autoFocus]);

    // Обработчик изменения размера для авторасширения
    const handleResize = React.useCallback(() => {
      if (!autoResize || !inputRef.current) return;

      const element = inputRef.current;
      const computedStyle = window.getComputedStyle(element);
      const lineHeight = parseInt(computedStyle.lineHeight) || 20;
      const paddingTop = parseInt(computedStyle.paddingTop) || 0;
      const paddingBottom = parseInt(computedStyle.paddingBottom) || 0;
      
      // Сбрасываем высоту для правильного расчета
      element.style.height = 'auto';
      
      // Вычисляем новую высоту
      const scrollHeight = element.scrollHeight;
      const newHeight = Math.min(
        scrollHeight,
        maxRows * lineHeight + paddingTop + paddingBottom
      );
      
      element.style.height = `${newHeight}px`;
    }, [autoResize, maxRows]);

    // Обработчик изменения значения
    const handleChange = React.useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
      if (autoResize) {
        // Небольшая задержка для правильного расчета
        setTimeout(handleResize, 0);
      }
      // Вызываем оригинальный onChange если он есть
      if (onChange) {
        onChange(e);
      }
    }, [autoResize, handleResize, onChange]);

    return (
      <input
        type={type}
        className={cn(
          "flex w-full rounded-md border border-input bg-background px-3 py-2 text-base ring-offset-background file:border-0 file:bg-transparent file:text-sm file:font-medium file:text-foreground placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50 md:text-sm",
          autoResize && "min-h-[40px] max-h-[120px] resize-none overflow-hidden",
          !autoResize && "h-10",
          className
        )}
        ref={inputRef}
        onChange={handleChange}
        onInput={autoResize ? handleResize : undefined}
        {...props}
      />
    )
  }
)
Input.displayName = "Input"

export { Input }