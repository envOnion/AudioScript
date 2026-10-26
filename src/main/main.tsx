import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import '@/shared/theme.css'
import { App } from './App'

const root = document.getElementById('root')
if (!root) throw new Error('Элемент #root не найден')

createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
)
