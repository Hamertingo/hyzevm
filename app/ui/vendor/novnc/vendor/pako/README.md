      short.push(['typeerror', er.stack])
      detail.push([
        'typeerror',
        [
          'This is an error with npm itself. Please report this error at:',
          '    https://github.com/npm/cli/issues',
        ].join('\n'),
      ])
      break

    default:
      short.push(['', er