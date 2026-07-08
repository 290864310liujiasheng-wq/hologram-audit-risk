function getOrders(uid) {
  return db.query(`SELECT * FROM orders WHERE user_id = ${uid}`);
}
