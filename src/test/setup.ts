import '@testing-library/jest-dom/vitest'
import { cleanup } from '@testing-library/react'
import { afterEach } from 'vitest'

afterEach(() => cleanup())

Object.defineProperty(window, 'confirm', { configurable: true, value: () => true })
Element.prototype.scrollIntoView = () => {}
