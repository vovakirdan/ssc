import ReactDOM from 'react-dom/client';
import App from './App';
import './style.css';
const root = document.getElementById('root');
if (!root) throw new Error('Missing application root');
ReactDOM.createRoot(root).render(<App />);
