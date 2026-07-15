import logging

async def process_payment(amount):
    try:
        result = await payment_api.charge(amount)
        return result
    except PaymentError as e:
        logging.error("Payment failed", exc_info=True)
        raise  # 重新抛出让调用方处理

async def delete_user(user_id):
    try:
        await db.delete(user_id)
    except DatabaseError as e:
        logging.error(f"Failed to delete user {user_id}: {e}")
        raise DatabaseError(f"User deletion failed: {e}") from e
