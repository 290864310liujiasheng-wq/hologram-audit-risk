class Query:
    def execute(self):
        return self.runner.run(self.plan)
