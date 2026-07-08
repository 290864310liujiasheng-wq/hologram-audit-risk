def get_user(user_id):
    cursor.execute(f"SELECT * FROM users WHERE id = {user_id}")
