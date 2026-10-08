import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App.tsx';
import DesktopHome from './DesktopHome.tsx';
import './index.css';

const isReview = location.pathname.split('/').filter(Boolean)[0] === 'review';

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>{isReview ? <App /> : <DesktopHome />}</React.StrictMode>,
);
