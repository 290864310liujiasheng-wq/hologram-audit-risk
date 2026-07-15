import asyncio

async def process_payment(amount):
    try:
        result = await payment_api.charge(amount)
        return result
    except Exception:
        pass  # AI-003: 静默吞掉支付错误

async def delete_user(user_id):
    try:
        await db.delete(user_id)
    except Exception as e:
        print(f"error: {e}")  # AI-003: 只打印不处理，调用方不知道失败了
