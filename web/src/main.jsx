import React from 'react'
import { createRoot } from 'react-dom/client'
import App from './App.jsx'
import { setTokenPrompt } from './api.js'
import './style.css'

setTokenPrompt(async ({ rejected }) =>
  window.prompt(rejected ? 'That admin token was refused. Admin token:' : 'Admin token for changes:')
)

createRoot(document.getElementById('root')).render(<App />)
