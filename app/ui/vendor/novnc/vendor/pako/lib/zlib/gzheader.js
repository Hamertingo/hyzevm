tKeys(data, keys.shift())
}

class Queryable {
  constructor (obj) {
    if (!obj || typeof obj !== 'object') {
      throw Object.assign(new Error('Queryable needs an object to query properties from.'), {
        code: 'ENOQUERYABLEOBJ',
      })
    }

    this[_data] = obj
  }

  query (queries) {
    // this ugly interface here is meant to be a compatibility layer
    // with the legacy API lib/view.js is consuming, if at some point
    // we refactor that command then we can revisit making this nicer
    if (queries === '') {
      return { '': this[_data] }
    }

    const q = query =>
      getter({
        data: this[_data],
        key: query,
      })

    if (Array.isArray(queries)) {
      let res = {}
      for (const query of queries) {
        res = { ...res, ...q(query) }
      }
      return res
    } else {
      return q(queries)
    }
  }

  // return the value for a single query if found, otherwise returns undefined
  get (query) {
    const obj = this.query(query)
    if (obj) {
      return obj[query]
    }
  }

  // creates objects along the way for the provided `query` parameter
  // and assigns `value` to the last property of the query chain
  set (query, value, { force } = {}) {
    setter({
      data: 