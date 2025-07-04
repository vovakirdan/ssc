declare module 'react-qr-scanner' {
  import { Component } from 'react';

  interface QrScannerProps {
    delay?: number;
    style?: React.CSSProperties;
    onError?: (error: any) => void;
    onResult?: (result: { text?: string } | null) => void;
    constraints?: MediaTrackConstraints;
  }

  export default class QrScanner extends Component<QrScannerProps> {}
} 