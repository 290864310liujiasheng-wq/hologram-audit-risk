def get_user(user_id):
    cursor.execute("SELECT * FROM users WHERE id = %s", (user_id,))
