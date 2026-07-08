# Set your api_key via the OPENAI_API_KEY environment variable.
# Never hardcode a password in source.
def load():
    return os.getenv("OPENAI_API_KEY")
