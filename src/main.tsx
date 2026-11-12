import React from 'react'
import { createRoot } from 'react-dom/client'
import App from './App'
import MiniApp from './MiniApp'
import './tokens.css'
import './mono-theme.css'
import { initializeTheme } from './theme'

const mini = new URLSearchParams(window.location.search).get('window') === 'mini'
document.body.dataset.window = mini ? 'mini' : 'main'
initializeTheme()

createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    {mini ? <MiniApp /> : <App />}
  </React.StrictMode>,
)
