import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Shield, MessageCircle, Users, ArrowRight } from 'lucide-react';

interface WelcomeProps {
  onStart: () => void;
}

const Welcome = ({ onStart }: WelcomeProps) => {
  return (
    <div className="min-h-screen bg-gradient-to-br from-slate-900 via-slate-800 to-slate-900 flex items-center justify-center p-4">
      <div className="max-w-md w-full space-y-6">
        <div className="text-center space-y-4">
          <div className="mx-auto w-20 h-20 bg-gradient-to-r from-emerald-500 to-cyan-500 rounded-full flex items-center justify-center">
            <Shield className="w-10 h-10 text-white" />
          </div>
          <h1 className="text-3xl font-bold text-white">SecureChat</h1>
          <p className="text-slate-300 text-lg">
            Супер секретный чат с end-to-end шифрованием
          </p>
        </div>

        <div className="space-y-4">
          <Card className="bg-slate-800/50 border-slate-700">
            <CardContent className="p-4 flex items-center space-x-3">
              <MessageCircle className="w-8 h-8 text-emerald-500" />
              <div>
                <h3 className="text-white font-semibold">Приватные сообщения</h3>
                <p className="text-slate-400 text-sm">Никто не может прочитать ваши сообщения</p>
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