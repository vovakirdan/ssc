import { Button } from '@/components/ui/button';
import { Card, CardContent } from '@/components/ui/card';
import { Shield, MessageCircle, Users, ArrowRight } from 'lucide-react';
import DecryptedText from '@/components/text/DecryptedText';
import TrueFocus from '@/components/text/TrueFocus';
import { useEffect, useState } from 'react';

interface WelcomeProps {
  onStart: () => void;
}

const Welcome = ({ onStart }: WelcomeProps) => {
  // Состояние для перезапуска анимации
  const [animationKey, setAnimationKey] = useState(0);

  // Эффект для зацикливания анимации каждые 3 секунды
  useEffect(() => {
    const interval = setInterval(() => {
      setAnimationKey(prev => prev + 1);
    }, 5000);

    return () => clearInterval(interval);
  }, []);

  return (
    <div className="min-h-screen bg-gradient-to-br from-slate-900 via-slate-800 to-slate-900 flex items-center justify-center p-4">
      <div className="max-w-md w-full space-y-6">
        <div className="text-center space-y-4">
          <div className="mx-auto w-20 h-20 bg-gradient-to-r from-emerald-500 to-cyan-500 rounded-full flex items-center justify-center">
            <Shield className="w-10 h-10 text-white" />
          </div>
          <h1 className="text-3xl font-bold text-white">
            <TrueFocus
              sentence="Secure Chat"
              manualMode={false}
              blurAmount={5}
              borderColor="green"
              glowColor="rgba(0, 255, 0, 0.6)"
              animationDuration={0.8}
              pauseBetweenAnimations={2}
            />
          </h1>
          <p className="text-slate-300 text-lg">
            Супер секретный чат с end-to-end шифрованием
          </p>
        </div>

        <div className="space-y-4">
          <Card className="bg-slate-800/50 border-slate-700">
            <CardContent className="p-4 flex items-center space-x-3">
              <MessageCircle className="w-8 h-8 text-emerald-500" />
              <div>
                <h3 className="text-white font-semibold">
                  <DecryptedText
                    key={animationKey} // Ключ для принудительного пересоздания компонента
                    text="Приватные сообщения"
                    speed={100}
                    animateOn="view"
                  />
                </h3>
                <p className="text-slate-400 text-sm">
                  <DecryptedText
                    key={animationKey + 1} // Разный ключ для второго компонента
                    text="Никто не может прочитать ваши сообщения"
                    speed={100}
                    animateOn="view"
                  />
                </p>
              </div>
            </CardContent>
          </Card>

          <Card className="bg-slate-800/50 border-slate-700">
            <CardContent className="p-4 flex items-center space-x-3">
              <Users className="w-8 h-8 text-cyan-500" />
              <div>
                <h3 className="text-white font-semibold">P2P соединение</h3>
                <p className="text-slate-400 text-sm">Прямое соединение без серверов</p>
              </div>
            </CardContent>
          </Card>
        </div>

        <Button 
          onClick={onStart}
          className="w-full bg-gradient-to-r from-emerald-500 to-cyan-500 hover:from-emerald-600 hover:to-cyan-600 text-white font-semibold py-3 text-lg"
        >
          Начать чат
          <ArrowRight className="w-5 h-5 ml-2" />
        </Button>

        <p className="text-center text-slate-500 text-sm">
          Для начала чата создайте QR-код или отсканируйте существующий
        </p>
      </div>
    </div>
  );
};

export default Welcome;