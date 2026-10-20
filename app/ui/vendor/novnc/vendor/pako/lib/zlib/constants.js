const chalk = require('chalk')
const nocolor = {
  bold: s => s,
  dim: s => s,
  red: s => s,
  yellow: s => s,
  cyan: s => s,
  magenta: s => s,
  blue: s => s,
  green: s => s,
  gray: s => s,
}

const { relative } = require('path')

const explainNode = (node, depth, color) =>
  printNode(node, color) +
  explainDependents(node, depth, color) +
  explainLinksIn(node, depth, color)

const colorType = (type, color) => {
  const { red, yellow, cyan, magenta, blue, green, gray } = color ? chalk : nocolor
  const style = type === 'extraneous' ? red
    : type === 'dev' ? yellow
    : type === 'optional' ? cyan
    : type === 'peer' ? magenta
    : type === 'bundled' ? blue
    : type === 'workspace' ? green
    : type === 'overridden' ? gray
    : /* istanbul ignore next */ s => s
  return style(type)
}

const printNode = (node, color) => {
  const {
    name,
    version,
    location,
    extraneous,
    dev,
    optional,
    peer,
    bundled,
    isWorkspace,
    overridden,
  } = node
  const { bold, dim, green } = color ? chalk : nocolor
  const extra = []
  if (extraneous) {
    extra.push(' ' + bold(colorType('extraneous', color)))
  }

  if (dev) {
    extra.push(' ' + bold(colorType('dev', color)))
  }

  if (optional) {
    extra.push(' ' + bold(colorType('optional', color)))
  }

  if (peer) {
    ext