async function saveData(data) {
  try {
    await db.insert(data);
  } catch (e) {}  // AI-003: 空 catch

  try {
    await cache.set(data.id, data);
  } catch (error) {
    console.log(error);  // AI-003: 只打印
  }
}
