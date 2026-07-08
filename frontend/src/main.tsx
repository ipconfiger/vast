import React from 'react'
import { createRoot } from 'react-dom/client'
import App from './App'
import './index.css'

if ('serviceWorker' in navigator) {
    navigator.serviceWorker.register('/sw.js').then((reg) => {
        console.info('[SW] registered, scope:', reg.scope);
    }).catch((err) => {
        console.warn('[SW] registration failed:', err);
    });
}

createRoot(document.getElementById('root')!).render(
    <React.StrictMode>
        <App />
    </React.StrictMode>
)
